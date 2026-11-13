//! Live Telegram resolver tests (network). Public post: https://t.me/telegram/193

#![cfg(feature = "telegram")]

use jyroxa_core::{build_client, resolve_post};

#[tokio::test]
async fn live_resolve_public_telegram_video() {
  if std::env::var("TELEGRAM_LIVE_TEST").ok().as_deref() != Some("1") {
    eprintln!("skip live_resolve_public_telegram_video (set TELEGRAM_LIVE_TEST=1)");
    return;
  }
  let client = build_client().expect("client");
  let url = "https://t.me/telegram/193";
  let items = resolve_post(&client, url)
    .await
    .expect("resolve public embed");
  assert!(!items.is_empty());
  let first = &items[0];
  assert!(!first.download_url.is_empty());
  assert!(!first.filename.is_empty());
  let resp = client.get(&first.download_url).send().await.expect("cdn get");
  assert!(resp.status().is_success());
}
