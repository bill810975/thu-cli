# Library and sports service coverage

Library support is ported from `packages/thu-info-lib/src/lib/library.ts`. The
retired sports `gymbook` adapter has been replaced by a native client for the
current venue platform at `https://www.sports.tsinghua.edu.cn/venue/`. Every
request still requires a valid Tsinghua identity session and the venue service's
own SSO exchange.

## Library operations

| Upstream | CLI operation | Behavior |
|---|---|---|
| getLibraryList | library.list | Native request, returns `data.list` |
| getLibraryFloorList | library.floors | Native floor tree; does not aggregate availability across every floor |
| getLibrarySectionList | library.sections | Native sections and availability payload |
| getLibrarySeatList | library.seats | Fetch date segment, then seats; current time uses UTC+8; raw seat schema |
| internal getLibraryDay | library.days | All dates and segments |
| bookLibrarySeat | library.book | Fetch selected day's segment and library access token; send booking |
| getBookingRecords | library.records | Parse table and cancellation IDs |
| cancelBooking | library.cancel | Fetch token, send deletion form, check server status |
| toggleSocketState | library.socket-update | Update crowdsourced socket availability, not physical outlet control |
| socket status in seat list | library.socket-status | Separate optional request, not merged into seat results |
| getAccNo | library.room-user | Fetch live CAB account instead of in-memory cached account ID |
| getLibraryRoomBookingInfoList | library.room-list | Native CAB request |
| getLibraryRoomBookingResourceList | library.room-resources | Native resource payload including rules and bookings |
| fuzzySearchLibraryId | library.room-members | Search first ten member accounts, as upstream |
| bookLibraryRoom | library.room-book | Fetch current account, submit explicit members/device/time |
| getLibraryRoomBookingRecord | library.room-records | Explicit date range instead of automatic next six days |
| cancelLibraryRoomBooking | library.room-cancel | Native JSON cancellation |
| updateLibraryRoomEmail | library.room-email | Native email update |
| cabLogin | Session establishment | Implemented by shared auth `roam` with policy `cab`, dynamically obtaining the identity payload; valid service cookies may also be imported |

Room member IDs are **CAB `accNo`**, not university student numbers. Obtain them with `library.room-members`. `library.room-book` accepts a JSON numeric array. Seat and room booking do not automatically retry writes.

## Sports operations

The current service performs a one-time SSO token exchange. The venue token is
kept in the private mode-600 `sports-auth` sidecar beside the selected Info
profile. Login, cookie import, logout, or a profile identity change invalidates
that sidecar.

| CLI operation | Behavior |
|---|---|
| sports.auth-status | Verify the venue account and SSO session |
| sports.scenes | List current venues/scenes; `query` filters by name |
| sports.areas | List device kinds, buildings, and rooms for a scene |
| sports.site-types | List the site-kind dictionary used by `site_kind_id` |
| sports.scene-detail | Read venue metadata and reservation mode |
| sports.availability | Return compact bookable courts/sessions for a date; unavailable sessions are filtered by default |
| sports.pay-types | Read enabled online payment channels and supported booking payment modes |
| sports.records | List the authenticated user's reservations and orders without member identity data |
| sports.preflight | Resolve one exact site/session and validate its form, payment mode, price, and captcha requirement without writing |
| sports.book | Revalidate availability, lock the site, create one reservation, and create/check the order; it never completes payment |
| sports.cancel | Cancel one explicitly identified reservation |

The current frontend maps fee `payType` values `1` and `3` to `PAY_ONLINE`, and
`2` to `PAY_OFFLINE`. `PAY_CARD` is available only when the venue exposes a
usable card. Availability output intentionally omits reservation-holder details
and raw frontend payloads.

The safe sequence is `scenes` → `availability` → `preflight` → explicit user
confirmation → `book`. `sports.book` requires `--confirm`; a cancelled or failed
`addReserve` releases a previously obtained short lock. `payment_completed` in
the result is always false. If an online order is generated, complete payment
in the official venue/order page before its hold expires. Never retry an
ambiguous booking response without checking `sports.records`.

## Examples

Pass JSON using the top-level CLI's service-call command, e.g. operation `library.seats` with `{"section_id":123,"date":"2026-11-12"}`. Consult `thu-cli --help` for the exact command syntax.

- Seat booking arguments: `{"seat_id":123,"section_id":45,"user_id":"2023...","type":1,"date":"2026-11-12"}`.
- Room booking: `{"device_id":123,"start":"2026-11-12 10:00","end":"2026-11-12 12:00","members":[123456]}`.
- Sports scenes: `thu-cli thu sports.scenes --args '{"query":"网球"}'`.
- Sports availability: `{"scene_uuid":"SCENE_UUID","date":"2026-11-12"}`.
- Sports preflight: `{"scene_uuid":"SCENE_UUID","site_uuid":"SITE_UUID","start":"2026-11-12 18:00","end":"2026-11-12 19:00","pay_type":"PAY_ONLINE"}`.
- Sports booking: use the exact UUIDs and normalized start/end returned by availability, repeat the preflight, then add `--confirm`.

The CLI does not solve captchas or approval requirements. If preflight reports a
captcha or custom form, complete that requirement through explicit arguments
only when the current frontend contract supports it; otherwise stop rather than
race or guess. No automatic retries are implemented for locks, bookings, or
cancellations.

The core CLI must guard all operations whose metadata has `mutating:true`. Service-specific business restrictions remain enforced by the university. Offline parser tests do not establish that live page layouts or policy remain unchanged.

## Classroom and meeting-room reservations

The current service is `https://jxlytj.tsinghua.edu.cn/site`. It uses Tsinghua
unified identity, per-request signing, and encrypted JSON responses. The CLI
keeps the rotating platform token in the private mode-600 `jxlytj-auth`
sidecar.

The signing contract is:

```text
appId + timeStamp + random 32-character nonce
MD5("appId=...&nonce=...&timeStamp=...&key=...")
```

Encrypted responses use AES-256-CBC with the frontend signing key, the
16-character ASCII IV `0000000000000000`, and CryptoJS `Iso10126` padding.

Start with:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu classroom-reservation.auth-status

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu classroom-reservation.scenes

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu classroom-reservation.sites \
  --args '{"scene_uuid":"SCENE_UUID"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu classroom-reservation.records

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu classroom-reservation.history
```

Availability operations separate same-day, cross-day, period, quick, and
custom-time queries. `current`, `cross-day`, and their custom variants accept
the exact server-shaped `payload`; `recommend` accepts the frontend query
object.

Writes are explicit and guarded:

| CLI operation | Effect |
|---|---|
| classroom-reservation.lock | Acquire the short frontend room/period lock |
| classroom-reservation.unlock | Release a known lock |
| classroom-reservation.book | Create one fixed-period reservation |
| classroom-reservation.book-custom | Create one custom-time reservation |
| classroom-reservation.cancel | Cancel one reservation |
| classroom-reservation.cancel-period | Cancel one period |
| classroom-reservation.cancel-batch | Cancel selected reservations |
| classroom-reservation.detail-update | Update a reservation |
| classroom-reservation.end-early | End an active reservation early |
| classroom-reservation.extend | Extend an active reservation |

Review `records` or `detail` before repeating an ambiguous write.

Live reads verified authentication, three scenes, scene details, site types,
campus/building selection, same/cross-day queries, reservation records,
current-year history, recommended rooms, common rooms, equipment, room
dictionaries, user credit, lock configuration, groups, and system
configuration.

The browser sends these reads as same-origin requests with `Accept`,
`Language-Set`, `Referer`, and `token`, but without a cross-origin `Origin` or
the generic `x-api-version` header. Replaying `api/site/choose` with the wrong
header set returned 404 even though the browser request returned 200. The
native client now mirrors the browser request headers.

## Campus visitor appointments

The `预约入校` family spans two systems:

| Card | System | Stable identifiers |
|---|---|---|
| 临时来访报备 | `visit.tsinghua.edu.cn` | `yyfwid=C52889552AA7F89258B3A38DF9DBAF27` |
| 学生访客（无身份证人员）预约 | `thos.tsinghua.edu.cn` | service `611741148012544` |
| 亲友来访人员报备 | `thos.tsinghua.edu.cn` | service `27277790547968` |

### Temporary-visitor reporting

The CLI obtains a one-use campus code, initializes the visitor session, and
exchanges the callback ticket through the multipart `identity/ssoLogin` route.
The resulting token remains in the private mode-600 `visit-auth` sidecar.

Read `visit.menus`, `visit.unread-count`, `visit.task-records`, or
`visit.form-preload` before constructing a submission. `visit.form-submit`
requires a fully reviewed payload and `--confirm`.

The current browser requests the legacy `portal/getSecAppInfo` endpoint, receives
HTTP 404, and then recovers by requesting `portal/getPortlets?portalUuid=`.
`visit.portal` calls the replacement endpoint directly so the CLI does not
surface the browser's expected, non-fatal 404.

### THOS visitor forms

Use `visit.thos-enter` or the read-only operations:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu visit.thos-service-status \
  --args '{"workflow":"student_visitor"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu visit.thos-apply-info \
  --args '{"workflow":"student_visitor"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu visit.thos-form-load \
  --args '{"workflow":"student_visitor"}'
```

The service and form IDs are:

| Workflow | FORMID | WORKFLOWTEMPID |
|---|---|---|
| student_visitor | `b97336a8-8c2c-4b55-8f21-48b960db` | `af48e03d-3ced-4bf2-9bbb-b8939e8af3e3` |
| relative_visitor | `a33a1092-b8bd-49a3-be61-38b41ed8` | `1eb41f5a-80d0-4876-b7bd-98f334802d3d` |

THOS checks service status through:

```text
POST /fp/fp/serveapply/checkService
POST /fp/fp/serveapply/checkLimittime
POST /fp/fp/serveapply/checkServiceSyncLimit
POST /fp/fp/serveapply/serveInfo
POST /fp/fp/serveapply/getServeApply
```

The actual form is read through:

```text
GET /fp/formParser?status=select&formid=...&service_id=...&process=...
```

Both visitor forms were loaded successfully through `visit.thos-form-load`.
That operation returns only a content summary and does not expose full form
contents. THOS form submission is not implemented as a separate write because
the Matrix form runtime assembles variables and file lists client-side; a
guessed request would be unsafe. No classroom lock, booking, cancellation, or
visitor submission was created during verification.
