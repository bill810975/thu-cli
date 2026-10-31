# Library and sports service coverage

Native adapters derived from `packages/thu-info-lib/src/lib/library.ts` and `sports.ts` in thu-info-app. Every request requires a valid session for its specific service. A WebVPN cookie alone does not prove that seat, CAB room, and sports SSO sessions exist. No authenticated production calls were made during development.

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

| Upstream | CLI operation | Behavior |
|---|---|---|
| sportsIdInfoList | sports.venues | Eight upstream venue/item pairs; availability may change |
| getSportsResources | sports.resources | Fetch limits, phone, resource hashes, price and status; parse upstream script syntax without executing JS |
| internal getSportsPhoneNumber | sports.phone | Native contact read |
| updateSportsPhoneNumber | sports.phone-update | Validate mobile format, native update |
| getSportsCaptchaUrlMethod | sports.captcha | Returns image URL; fetch with the same cookie session and solve manually |
| getSportsReservationRecords | sports.records | Parse paid and unpaid tables, payment and cancellation IDs |
| makeSportsReservation | sports.book | Create order only, **never auto-pay**; server success message required |
| unsubscribeSportsReservation | sports.cancel | Native cancellation |
| paySportsReservation | sports.payment-qr | Native multi-step ID/token check, form submission and Alipay QR extraction; manual final payment |
| alternative handoff | sports.payment-start | Prepare first payment form only |

`sports.payment-qr` implements the upstream funds-platform ID/token check and QR extraction chain. It only accepts HTTPS form actions on tsinghua.edu.cn or alipay.com domains. A newly introduced payment provider fails closed for manual continuation. `sports.payment-start` is an alternative that stops after the first form. Neither completes a payment. A prepared payment form is sensitive, transient data; do not save or publish it. Review amount and finish payment yourself in Alipay. The full live chain has not been authenticated-tested.

## Examples

Pass JSON using the top-level CLI's service-call command, e.g. operation `library.seats` with `{"section_id":123,"date":"2026-11-12"}`. Consult `thu-cli --help` for the exact command syntax.

- Seat booking arguments: `{"seat_id":123,"section_id":45,"user_id":"2023...","type":1,"date":"2026-11-12"}`.
- Room booking: `{"device_id":123,"start":"2026-11-12 10:00","end":"2026-11-12 12:00","members":[123456]}`.
- Sports resources: `{"gym_id":"3998000","item_id":"4045681","date":"2026-11-12"}`.
- Sports booking: use the current `resHash` from resources, explicit price, phone and human-entered captcha. A stale hash/captcha can fail. No racing, automated captcha solving or automatic retries are implemented.

The core CLI must guard all operations whose metadata has `mutating:true`. Service-specific business restrictions remain enforced by the university. Offline parser tests do not establish that live page layouts or policy remain unchanged.
