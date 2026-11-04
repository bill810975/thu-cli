# Native authentication and session lifecycle

The CLI implements the upstream credential protocols in Rust. It does not shell
out to Node, Python, OpenSSL, a mobile app, or a browser for ordinary login.

## Implemented

- Unified ID form retrieval and `#sm2publicKey` parsing.
- Native SM2 encryption, including the exact `04 || C1 || C3 || C2` wire layout
  expected by JavaScript `sm-crypto`. A generated-key encrypt/decrypt test validates
  the byte ordering and Unicode password length handling.
- ID credential POST fields `i_user`/`i_pass`, fingerprint and CAPTCHA field;
  account-settings uses `username`/`password` instead.
- Info/WebVPN login, identity callback, separate information-portal roaming, then
  verification that the portal supplies an XSRF token. WebVPN login alone is not
  reported as a complete information-portal session.
- Direct Learn login with multipart credential form, ticket exchange and target
  page CSRF verification.
- Interactive native second-factor discovery, code request and verification for
  `mobile` (SMS), `wechat`, and `totp`. `sms` is accepted as a CLI alias for `mobile`.
  Codes are prompted without terminal echo. No MFA is bypassed.
- Roaming policies `default`, `id`, `id_website`, `card`, `cab`, `cr`, `gitlab`;
  known WebVPN hostname mapping, invoice ticket handling and madmodel ticket check.
- CAB library-room auth-address discovery with empty payload, account bootstrap,
  and validation of the returned PID when a username was supplied.
- Existing trusted-form parsing is supported. With `--trust-device`, login
  requests the persistent `singleLogin` session and registers the stable CLI
  fingerprint through `saveFinger` after successful interactive 2FA.
- The live identity service can issue a persistent `TSINGHUAUSERID` Cookie for
  `singleLogin`. `--refresh --non-interactive` reuses this Cookie without a
  password, GUI prompt or Keychain access.
- Successful login stores non-secret account and route metadata in a private
  sidecar next to the Cookie profile. Read-only operations use it to refresh an
  expired trusted-device session on demand, then retry once. No background
  daemon or scheduled keepalive is required.
- The reimbursement service exchanges an Info portal ticket for its own rotating
  API token. That token is stored in a separate mode-`600` sidecar and refreshed
  on demand. The CLI does not extract browser Cookies or run a token daemon.
- macOS Keychain remains an opt-in fallback for the case where the persistent
  identity Cookie expires and unattended password reauthentication is required.
- Password memory owned by LoginOptions is zeroized on drop. HTTP bodies and
  authentication ticket URLs are not printed in errors.

## Running locally

Use `thu-cli login --username YOUR_STUDENT_ID --service info` (or `learn`). Password
input is hidden. The core session layer persists Cookies. The password remains
unpersisted unless the user explicitly stores it in macOS Keychain.
When the identity service requests verification, select a supported method and
enter the code locally. A service may need separate SSO roaming; pass the username
when required so the CLI can prompt for the password again. Use separate session
files for separate accounts. Re-run login to renew an expired session.

The CLI does not silently replay mutations after authentication errors. A timeout
can mean a mutation already executed; inspect server state before retrying.

Automatic recovery is deliberately asymmetric:

- Read-only campus operations try the direct endpoint first. If that fails, the
  CLI may refresh the trusted-device session, roam into the destination system,
  and retry once. This permits campus intranet use without a WebVPN ticket.
- State-changing operations establish authentication and destination roaming
  before execution. If their request then reports an authentication failure,
  the CLI does not replay it because the server may already have committed the
  change.
- `--no-auto-refresh` disables only this recovery path for an individual call.
- Automatic refresh remains best-effort. If the trusted device is no longer
  accepted, the CLI fails rather than prompting inside a background or automated
  call. An optional macOS Keychain password can extend unattended recovery when
  no interactive MFA challenge is required.

## Deliberate limits and validation boundary

- No real Tsinghua credentials were available during development. Native protocol
  code, parsers and encryption tests do not establish authenticated production
  compatibility. School-side changes, account entitlements and network access can
  still prevent login or service calls.
- The upstream libraries submit an empty `i_captcha`. This version detects CAPTCHA
  challenge responses and stops rather than attempting to solve or bypass them.
  Image CAPTCHA completion / browser-session import is a documented fallback,
  not a native image challenge implementation.
- Native WebAuthn/passkey UI and browser-generated risk-device tokens are not
  implemented. No spoofed device attestation or anti-bot bypass is attempted.
- Trusted-browser persistence depends on the school accepting `singleLogin` and
  the trusted fingerprint; cross-system identity prompts can still recur.
- Keychain refresh is opt-in. A remaining MFA, CAPTCHA, passkey or server-side
  trust invalidation still requires a human when automatic verification cannot
  proceed.
- External browser login/CAPTCHA completion requires the user's own browser. The
  CLI must not claim a profile is authenticated merely because cookies exist.

Sources: pinned `thu-info-app` checkout, `packages/thu-info-lib/src/lib/core.ts`,
`src/lib/library.ts`, `src/constants/strings.ts`; pinned `thu-learn-lib/src/index.ts`.

## Explicit account-management operations

The THU catalog also exposes passkey list/remove/rename and trusted-device
list/remove. Mutations require the CLI confirmation flag and an authenticated
account-settings session. These requests were not executed during development.
Native passkey key generation, hardware signing and enrollment remain unsupported.

`device.forget-thu-info` ports the upstream `forgetDevice` algorithm, including its
maximum of ten removals. It can remove multiple devices whose names begin with
`THU Info APP`; it does **not** mean “remove only this CLI device.” Prefer the
explicit `device.remove` with a selected UUID. A partial failure may follow earlier
successful removals; inspect the device list rather than blindly retrying.
