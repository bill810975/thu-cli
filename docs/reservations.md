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
