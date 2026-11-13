//! MTProto client (grammers) for private Telegram posts.

use std::path::Path;
use std::sync::Arc;

use grammers_client::client::{LoginToken, PasswordToken};
use grammers_client::media::{Document, Media};
use grammers_client::{Client, SignInError};
use grammers_mtsender::SenderPool;
use grammers_session::storages::MemorySession;
use grammers_session::types::PeerId;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::error::{Result, TelegramError};
use super::session::{export_memory_session, memory_from_persisted, PersistedTelegramSession, SessionStore};
use super::TelegramMediaItem;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TelegramAccountView {
    pub connected: bool,
    pub phone: Option<String>,
    pub display_name: Option<String>,
    pub api_id: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelegramLoginStep {
    NeedApiCredentials,
    NeedPhone,
    NeedCode { phone: String },
    NeedPassword,
    Done,
}

#[derive(Default)]
pub struct TelegramAuthState {
    pub api_id: Option<i32>,
    pub api_hash: Option<String>,
    pub phone: Option<String>,
    pending_token: Option<LoginToken>,
    pending_password: Option<PasswordToken>,
}

struct RunningPool {
    session: Arc<MemorySession>,
    client: Client,
    _runner: JoinHandle<()>,
    _updates: JoinHandle<()>,
}

/// Shared Telegram MTProto connection for the desktop shell.
pub struct TelegramClientPool {
    store: SessionStore,
    pool: Mutex<Option<RunningPool>>,
    auth: Mutex<TelegramAuthState>,
    meta: Mutex<Option<PersistedTelegramSession>>,
}

impl TelegramClientPool {
    pub fn new(data_dir: &std::path::Path) -> Self {
        TelegramClientPool {
            store: SessionStore::new(data_dir),
            pool: Mutex::new(None),
            auth: Mutex::new(TelegramAuthState::default()),
            meta: Mutex::new(None),
        }
    }

    pub async fn bootstrap(&self) -> Result<()> {
        if !self.store.exists() {
            return Ok(());
        }
        let persisted = self.store.load()?;
        self.spawn_pool(&persisted).await?;
        {
            let mut auth = self.auth.lock().await;
            auth.api_id = Some(persisted.api_id);
            auth.api_hash = Some(persisted.api_hash.clone());
            auth.phone = Some(persisted.phone.clone());
        }
        *self.meta.lock().await = Some(persisted);
        Ok(())
    }

    pub async fn account_view(&self) -> TelegramAccountView {
        let meta = self.meta.lock().await;
        let auth = self.auth.lock().await;
        let connected = self.pool.lock().await.is_some()
            && meta
                .as_ref()
                .map(|m| !m.dc_options.is_empty())
                .unwrap_or(false);
        TelegramAccountView {
            connected,
            phone: meta.as_ref().map(|m| m.phone.clone()).or_else(|| auth.phone.clone()),
            display_name: meta.as_ref().and_then(|m| m.display_name.clone()),
            api_id: auth.api_id.or_else(|| meta.as_ref().map(|m| m.api_id)),
        }
    }

    pub async fn set_api_credentials(&self, api_id: i32, api_hash: String) {
        let mut auth = self.auth.lock().await;
        auth.api_id = Some(api_id);
        auth.api_hash = Some(api_hash);
    }

    pub async fn begin_login(&self, phone: String) -> Result<TelegramLoginStep> {
        let (api_id, api_hash) = {
            let auth = self.auth.lock().await;
            match (auth.api_id, auth.api_hash.clone()) {
                (Some(id), Some(hash)) => (id, hash),
                _ => return Ok(TelegramLoginStep::NeedApiCredentials),
            }
        };

        self.shutdown_pool().await;

        let memory = MemorySession::default();
        let persisted = PersistedTelegramSession {
            api_id,
            api_hash: api_hash.clone(),
            phone: phone.clone(),
            display_name: None,
            home_dc: 0,
            dc_options: Vec::new(),
        };
        self.spawn_with_session(memory, &persisted).await?;

        let client = self.client().await?;
        let token = client
            .request_login_code(&phone, &api_hash)
            .await
            .map_err(TelegramError::from_invocation)?;
        {
            let mut auth = self.auth.lock().await;
            auth.phone = Some(phone.clone());
            auth.pending_token = Some(token);
        }
        Ok(TelegramLoginStep::NeedCode { phone })
    }

    pub async fn submit_code(&self, code: String) -> Result<TelegramLoginStep> {
        let token = {
            let mut auth = self.auth.lock().await;
            let token = auth
                .pending_token
                .take()
                .ok_or_else(|| TelegramError::other("login not started"))?;
            token
        };
        let client = self.client().await?;
        match client.sign_in(&token, &code).await {
            Ok(user) => {
                let name = user
                    .first_name()
                    .unwrap_or("Telegram user")
                    .to_string();
                self.finalize_login(name).await?;
                Ok(TelegramLoginStep::Done)
            }
            Err(SignInError::PasswordRequired(password_token)) => {
                let mut auth = self.auth.lock().await;
                auth.pending_password = Some(password_token);
                Ok(TelegramLoginStep::NeedPassword)
            }
            Err(SignInError::InvalidCode) => Err(TelegramError::other("invalid login code")),
            Err(SignInError::Other(e)) => Err(TelegramError::from_invocation(e)),
            Err(e) => Err(TelegramError::other(e.to_string())),
        }
    }

    pub async fn submit_password(&self, password: String) -> Result<TelegramLoginStep> {
        let password_token = {
            let mut auth = self.auth.lock().await;
            auth.pending_password
                .take()
                .ok_or_else(|| TelegramError::other("2FA required before password"))?
        };
        let client = self.client().await?;
        let user = client
            .check_password(password_token, password.as_bytes())
            .await
            .map_err(|e| TelegramError::other(e.to_string()))?;
        let name = user
            .first_name()
            .unwrap_or("Telegram user")
            .to_string();
        self.finalize_login(name).await?;
        Ok(TelegramLoginStep::Done)
    }

    async fn finalize_login(&self, display_name: String) -> Result<()> {
        let pool = self.pool.lock().await;
        let running = pool
            .as_ref()
            .ok_or_else(|| TelegramError::other("client not running"))?;
        let auth = self.auth.lock().await;
        let mut exported = export_memory_session(&running.session).await?;
        exported.api_id = auth.api_id.unwrap_or(0);
        exported.api_hash = auth.api_hash.clone().unwrap_or_default();
        exported.phone = auth.phone.clone().unwrap_or_default();
        exported.display_name = Some(display_name);
        self.store.save(&exported)?;
        *self.meta.lock().await = Some(exported);
        Ok(())
    }

    async fn shutdown_pool(&self) {
        if let Some(running) = self.pool.lock().await.take() {
            let _ = running.client.sign_out().await;
            running._runner.abort();
            running._updates.abort();
        }
    }

    pub async fn sign_out(&self) -> Result<()> {
        self.shutdown_pool().await;
        self.store.clear()?;
        *self.meta.lock().await = None;
        let mut auth = self.auth.lock().await;
        *auth = TelegramAuthState::default();
        Ok(())
    }

    pub async fn resolve_private_media(
        &self,
        chat_id: i64,
        message_id: u32,
        raw_url: &str,
    ) -> Result<Vec<TelegramMediaItem>> {
        let client = self.client().await?;
        let peer_id = PeerId::from_bot_api_dialog_id(chat_id)
            .ok_or_else(|| TelegramError::InvalidLink(format!("chat id {chat_id}")))?;
        let peer_ref = peer_id.to_ambient_ref();
        let _ = client
            .resolve_peer(peer_ref)
            .await
            .map_err(TelegramError::from_invocation)?;
        let messages = client
            .get_messages_by_id(peer_ref, &[message_id as i32])
            .await
            .map_err(TelegramError::from_invocation)?;
        let message = messages
            .into_iter()
            .next()
            .flatten()
            .ok_or(TelegramError::NotFound)?;
        let mut items = Vec::new();
        if let Some(media) = message.media() {
            items.push(media_item_from_grammers(&media, raw_url, chat_id, message_id)?);
        }
        if items.is_empty() {
            return Err(TelegramError::NoMedia);
        }
        Ok(items)
    }

    pub async fn download_private_media(
        &self,
        chat_id: i64,
        message_id: u32,
        output: &Path,
    ) -> Result<u64> {
        let client = self.client().await?;
        let peer_id = PeerId::from_bot_api_dialog_id(chat_id)
            .ok_or_else(|| TelegramError::InvalidLink(format!("chat id {chat_id}")))?;
        let peer_ref = peer_id.to_ambient_ref();
        let _ = client
            .resolve_peer(peer_ref)
            .await
            .map_err(TelegramError::from_invocation)?;
        let messages = client
            .get_messages_by_id(peer_ref, &[message_id as i32])
            .await
            .map_err(TelegramError::from_invocation)?;
        let message = messages
            .into_iter()
            .next()
            .flatten()
            .ok_or(TelegramError::NotFound)?;
        let media = message
            .media()
            .ok_or(TelegramError::NoMedia)?;
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        client
            .download_media(&media, output)
            .await
            .map_err(TelegramError::from_invocation)?;
        let size = std::fs::metadata(output)?.len();
        // Persist refreshed auth keys after large transfers.
        if let Some(running) = self.pool.lock().await.as_ref() {
            if let Some(mut meta) = self.meta.lock().await.clone() {
                let mut exported = export_memory_session(&running.session).await?;
                exported.api_id = meta.api_id;
                exported.api_hash = meta.api_hash.clone();
                exported.phone = meta.phone.clone();
                exported.display_name = meta.display_name.clone();
                let _ = self.store.save(&exported);
                meta.dc_options = exported.dc_options;
                *self.meta.lock().await = Some(meta);
            }
        }
        Ok(size)
    }

    async fn client(&self) -> Result<Client> {
        let guard = self.pool.lock().await;
        let running = guard
            .as_ref()
            .ok_or(TelegramError::LoginRequired)?;
        Ok(running.client.clone())
    }

    async fn spawn_pool(&self, persisted: &PersistedTelegramSession) -> Result<()> {
        let memory = memory_from_persisted(persisted);
        self.spawn_with_session(memory, persisted).await
    }

    async fn spawn_with_session(
        &self,
        session: MemorySession,
        persisted: &PersistedTelegramSession,
    ) -> Result<()> {
        self.shutdown_pool().await;
        let session = Arc::new(session);
        let pool = SenderPool::new(Arc::clone(&session), persisted.api_id);
        let client = Client::new(pool.handle);
        let runner = pool.runner;
        let mut updates = pool.updates;
        let runner_handle = tokio::spawn(async move {
            runner.run().await;
        });
        let updates_handle = tokio::spawn(async move {
            while updates.recv().await.is_some() {}
        });
        *self.pool.lock().await = Some(RunningPool {
            session,
            client,
            _runner: runner_handle,
            _updates: updates_handle,
        });
        Ok(())
    }
}

fn media_item_from_grammers(
    media: &Media,
    raw_url: &str,
    chat_id: i64,
    message_id: u32,
) -> Result<TelegramMediaItem> {
    let (filename, size, mime) = match media {
        Media::Document(doc) => document_meta(doc),
        Media::Photo(photo) => (
            format!("photo_{message_id}.jpg"),
            photo.size().map(|s| s as u64),
            Some("image/jpeg".into()),
        ),
        _ => (
            format!("telegram_{message_id}"),
            None,
            None,
        ),
    };
    Ok(TelegramMediaItem {
        download_url: String::new(),
        filename,
        size,
        mime,
        source_post_url: raw_url.to_string(),
        resumable: true,
        needs_login: true,
        private_chat_id: Some(chat_id),
        private_message_id: Some(message_id),
    })
}

fn document_meta(doc: &Document) -> (String, Option<u64>, Option<String>) {
    let filename = doc
        .name()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("document_{}.bin", doc.id()));
    let size = doc.size().map(|s| s as u64);
    let mime = doc.mime_type().map(|s| s.to_string());
    (filename, size, mime)
}

pub async fn resolve_private_media(
    pool: &TelegramClientPool,
    chat_id: i64,
    message_id: u32,
    raw_url: &str,
) -> Result<Vec<TelegramMediaItem>> {
    pool.resolve_private_media(chat_id, message_id, raw_url).await
}

pub async fn download_private_media(
    pool: &TelegramClientPool,
    chat_id: i64,
    message_id: u32,
    output: &Path,
) -> Result<u64> {
    pool.download_private_media(chat_id, message_id, output).await
}
