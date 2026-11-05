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
  `singleLogin`. `--refresh --non-interactive` first reuses this Cookie without a
  password or GUI prompt.
- Successful login stores non-secret account and route metadata in a private
  sidecar next to the Cookie profile. Read-only operations use it to refresh an
  expired trusted-device session on demand, then retry once. No background
  daemon or scheduled keepalive is required.
- The reimbursement service exchanges an Info portal ticket for its own rotating
  API token. That token is stored in a separate mode-`600` sidecar and refreshed
  on demand. The CLI does not extract browser Cookies or run a token daemon.
- If the identity server presents the password form despite the persistent
  Cookie, the CLI can use a matching account from a plaintext mode-`600`
  `credentials.json` beside the session profile. The credential file is
  re-read on demand for unattended renewal; Keychain is not used.
- Password memory owned by LoginOptions is zeroized on drop. HTTP bodies and
  authentication ticket URLs are not printed in errors.

## Why trusted-device renewal can fail

Tsinghua's identity login is not a conventional OAuth refresh-token system. The
CLI relies on campus session Cookies and a `singleLogin`/`TSINGHUAUSERID`
trusted-device Cookie. The Cookie can still be unexpired locally while the
identity server stops accepting it. In the observed failure, the local
`TSINGHUAUSERID` Cookie was valid through 2027, but the Info app login form
again contained `#sm2publicKey`, which means the server requested credentials
instead of issuing a passwordless redirect.

That can happen when the server invalidates the trusted device, does not treat
the saved fingerprint as trusted for this app, changes its device-risk policy,
or requires a fresh verification method. Local Cookie expiry cannot override
the server decision, and there is no durable refresh token to exchange.

There was also a client-side bug. The identity page contained a form carrying
`fingerPrint`, `fingerGenPrint`, and `fingerGenPrint3` even when it also
contained `#sm2publicKey`. The old `identity_response` code attempted the
trusted fingerprint form only when the public-key marker was absent, so it
skipped the passwordless path and went directly to
`identity credential password required`. The CLI now tries the trusted form
first whenever one is present, then falls back to a password if the response
still contains the SM2 public key.

Previously the password fallback was available only through an interactive
prompt or the optional macOS Keychain. When no Keychain entry existed,
unattended renewal had no remaining credential source. The local credential
file now provides the explicit fallback requested for this machine.

## Running locally

Use `thu-cli login --username YOUR_STUDENT_ID --service info` (or `learn`). Password
input is hidden. The core session layer persists Cookies. The password remains
unpersisted unless the user explicitly stores it in the local credential JSON:

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  credential set --username YOUR_STUDENT_ID --confirm
```

The file is `credentials.json` beside the selected session profile and contains
username/password entries in plaintext with mode `600`. The CLI rejects symlinks,
refuses group/other-readable files, writes atomically, and never prints a saved
password. `credential status` reports only whether an entry exists.
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
  accepted, the CLI uses the matching local credential when available. A
  remaining server-side MFA/CAPTCHA/passkey challenge still requires a human and
  is not bypassed.

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
- Stored-password refresh can complete only the password form. A remaining MFA,
  CAPTCHA, passkey or server-side trust invalidation still requires a human when
  automatic verification cannot proceed.
- Plaintext local credentials are protected only by filesystem permissions.
  Malware, an administrator, backup/sync software, or another process running as
  the same OS user can read them.
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
