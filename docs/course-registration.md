# Course Registration Protocol

This document records the course-registration operations exposed by `thu-cli`
and the upstream implementations used to reconstruct them.

## Sources

- [THU Info `cr.ts` at commit
  `7930cad`](https://github.com/thu-info-community/thu-info-app/blob/7930cad3f0d7243ad54df3ce4443b911113eef91/packages/thu-info-lib/src/lib/cr.ts),
  plus its
  [`models/cr/cr.ts`](https://github.com/thu-info-community/thu-info-app/blob/7930cad3f0d7243ad54df3ce4443b911113eef91/packages/thu-info-lib/src/models/cr/cr.ts).
- [OneTHU NextTHUxk at commit
  `045690b`](https://github.com/smartThise/NextTHUxk/tree/045690bf70bca01757829527dda58c7dd5f0bd4f)
  (2026-10-29), especially `src/data.js`, `src/config.js`, and
  `docs/ARCHITECTURE.md`.

The current service is the undergraduate `zhjwxk` application or its graduate
`xkYjs` counterpart. The current Info card opens the direct
`zhjwxk.cic.tsinghua.edu.cn/xklogin.do` endpoint, which redirects through
unified identity and returns a destination session through
`j_acegi_login.do`. This modern path does not add the legacy CAPTCHA layer.
The CLI uses the same direct unified-identity route and sends requests to the
direct registration host.

## Read Operations

| CLI operation | Backend request | Result |
| --- | --- | --- |
| `course.captcha` | GET `xklogin.do`, then GET `login-jcaptcah.jpg` | Legacy compatibility only; normal unified-identity login does not request this challenge |
| `course.semesters` | GET `xkBks.vxkBksXkbBs.do?m=main`, then `m=showTree` | Available undergraduate or graduate semesters |
| `course.plan` | GET `jhBks.vjhBksPyfakcbBs.do` or `jhYjs.vjhYjsGrpyfab.do` | Personal training-plan courses |
| `course.open` | POST `xkBks.vxkBksJxjhBs.do` with `m=kkxxSearch` | Open-course metadata, links, capacity and availability filters |
| `course.remaining` | POST the same endpoint with `m=kylSearch` | Capacity, remaining seats and queue size |
| `course.search` | Both of the preceding requests, merged on course and section | Normalized search result |
| `course.selected` | GET `xkBks.vxkBksXkbBs.do?m=yxSearchTab&tokenPriFlag=yx` | Current selected courses and normalized volunteer number |
| `course.stage` | GET `m=selectKc` with GBK-encoded `pathContent=一级选课` | Current stage, begin time and end time |
| `course.queue` | GET `m=dlSearch` | Courses currently in the waiting queue |
| `course.priority-meta` | GET `m=xkqkSearch` | Current and next volunteer-allocation stage |
| `course.priority` | GET/POST `xkBks.xkBksZytjb.do` with `tbzySearchBR` or `tbzySearchTy` | Volunteer statistics, optionally restricted to selected courses |
| `course.timetable` | Info portal deadline endpoint | Registration-related deadlines |
| `course.schedule` | GET `xkBks.vxkBksXkbBs.do?m=kbSearch` | Source timetable table rows |
| `course.levels` | GET the primary timetable with GBK-encoded `pathContent=一级课表` | Course type map (`必修`, `限选`, `任选`, `体育`) |
| `course.detail` | GET `js.vjsKcbBs.do?m=showToXs&p_id=TEACHER_ID;COURSE_ID` | Course description fields |
| `course.whole-schedule` | GET `syxk.vsyxkKcapb.do?m=ztkbSearch` | Selected and queued courses normalized from timetable cells |

Course search accepts course number, name, teacher, weekday, period,
department, grade, category, note, and undergraduate/graduate availability
filters. Chinese values are encoded as GBK, matching the legacy registration
application.

## Mutation Operations

Every operation below is marked `mutating:true` and requires the outer CLI
`--confirm`.

| CLI operation | Backend mutation | Safety behavior |
| --- | --- | --- |
| `course.select` | GET a category page for a fresh token, POST `saveBxKc`, `saveXxKc`, `saveRxKc`, or `saveTyKc` | If the server returns the queue-confirmation page, use its new one-time token and POST `saveBksKcDl` once |
| `course.delete` | POST `deleteYxk` for a selected course | `candidate:true` instead uses `dlSearchTab`, then POST `dlDelete` |
| `course.will` | POST `changeZY` | Requires the exact course number, section and target volunteer number |
| `course.pf-set` | POST `editpfyes` | Reads the live P/F form and selected row before posting |
| `course.pf-cancel` | POST `editpfcancle` | Reads the live P/F form and selected row before posting |

The CLI never retries an ambiguous mutation automatically. Check
`course.selected` and `course.queue` before repeating a select or delete after
a timeout or an unrecognized response.

## OneTHU-Specific Findings

The OneTHU NextTHUxk source supplies several details absent from the older THU
Info client:

- Course search is sent directly to `xkBks.vxkBksJxjhBs.do` with GBK-encoded
  Chinese filters.
- The selected-course table gained additional leading columns in 2026. The CLI
  therefore reads the authoritative course/section identity from `p_del_id` and
  keeps raw cells for diagnostics.
- Selected and queued course times can be corrected from `ztkbSearch`; that page
  is authoritative for weekday, period, teacher and week range.
- A full section can result in a second, token-changing confirmation request.
- A queued course is removed through `dlDelete`, not `deleteYxk`.
- WebVPN can return a generic shell page even while the main session is alive;
  a single re-entry at the site root can refresh the destination ticket.
- Local drafts, probability estimates, AI scheduling and conflict checks are
  client-side features, not additional registration-server operations.

## Verification Boundary

Offline tests cover the current catalog, selected-course column changes,
one-time queue confirmation, GBK filters, timetable parsing, direct unified-
identity routing and mutation classification. A live write test was not
performed.

On 2026-11-09, Chrome reproduced the current portal flow: Info opened the
direct `xklogin.do` SSO entry, unified identity completed successfully, and
the destination landed on `zhjwxk.cic.tsinghua.edu.cn/xkBks...?m=main` without
presenting a CAPTCHA. The CLI then switched to that direct route and was
verified with a read-only semester and selected-course query.
