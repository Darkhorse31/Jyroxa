# Telegram downloads in Jyroxa

Jyroxa can download media linked from Telegram posts. The implementation lives in `crates/jyroxa-core/src/telegram/` behind the Cargo feature `telegram`.

## Public posts (no login)

For URLs such as:

- `https://t.me/<channel>/<message_id>`
- `https://t.me/s/<channel>/<message_id>`
- `https://telegram.me/<channel>/<message_id>`

the resolver fetches the public embed page (`?embed=1&mode=tme`), parses video `src`, documents, and photos, then hands **direct HTTPS CDN URLs** to the normal segmented download engine. Albums (grouped media) return multiple files.

Example used in tests: [t.me/telegram/193](https://t.me/telegram/193) (official @telegram channel).

## Private posts (your account)

For `https://t.me/c/<chat_id>/<message_id>` (and other non-public posts), Telegram does not expose CDN URLs without authentication. Jyroxa uses the pure-Rust **grammers** MTProto client:

1. In **Settings → Telegram**, enter `api_id` and `api_hash` from [my.telegram.org](https://my.telegram.org/apps).
2. Enter your phone number, complete the login code (and 2FA password if enabled).
3. The session is stored encrypted with **Windows DPAPI** under your Jyroxa app data directory.

Downloads use Telegram’s file API (including parallel chunked transfers for large files via grammers). Only media your account can already open in the official app is accessible—there is no access-control bypass.

## Desktop integration

- **Add download** and **Link grabber** detect `t.me` links, probe filename/size, then queue downloads.
- Public items download over HTTPS; private items download over MTProto when signed in.

## Privacy

- API credentials and session keys stay on your machine (DPAPI-protected file).
- Jyroxa does not send your Telegram data to third-party servers beyond Telegram’s own API/CDN.
- Sign out in Settings removes the saved session file.

## Building

```powershell
. C:\Users\lenovo\.jyroxa-tools\env.ps1
cargo test -p jyroxa-core --features telegram
cargo clippy -p jyroxa-core --features telegram -- -D warnings
```

Live network test (optional):

```powershell
$env:TELEGRAM_LIVE_TEST = "1"
cargo test -p jyroxa-core --features telegram live_resolve_public_telegram_video
```

## Android / FFI

Not wired in this branch. See the project report for TODO.
