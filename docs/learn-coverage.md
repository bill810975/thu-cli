# learnX integration: provenance, coverage and verification boundary

## Sources

- learnX: https://github.com/robertying/learnX at `e6d81fbeb619b0837263688839b8e01c5c664ebe`.
- Its `pnpm-lock.yaml` resolves `thu-learn-lib` to `1b79b984f34a6f1ca0532beaf59abac55db7f52c`; this exact source was inspected at https://github.com/robertying/thu-learn-lib.
- Main contracts: `thu-learn-lib/src/index.ts`, `urls.ts`, `types.ts`, `utils.ts`; app-specific sharing: `learnX/src/helpers/coursex.ts`; app-specific multipart upload: `learnX/src/data/source.ts`.
- Both projects use MIT licenses. Required notices are included in `licenses/`.

This is a native Rust port of HTTP contracts and response processing. It does not call JavaScript, Node, Python, a hidden proxy service, or the mobile application. The original projects are clients of existing school services, not deployable copies of those servers.

## Commands and upstream methods

All commands use `thu-cli learn OPERATION --args '{...}'`. `thu-cli describe learn OPERATION` lists required and optional JSON arguments. `--args-file` is preferred for sensitive inputs. `role` defaults to `student`; `teacher` is supported only where the upstream method supports it, and does not grant teacher permissions.

| Upstream method | Operation | Implementation notes |
|---|---|---|
| `getRoamingTicket`, `login` | Core `login --service learn` | Separate native identity module; user completes required MFA/CAPTCHA. Credentials are not bundled. |
| `getCSRFToken`, `setCSRFToken` | Automatic discovery; optional `csrf` input | Extracted from authenticated homepage. No token-output command. |
| `logout` | `logout` | Remote Learn session invalidation, requires `--confirm`. Core top-level logout only clears local cookies. |
| `getUserInfo` | `user` | Parses name and department from homepage selectors. |
| `getSemesterIdList` | `semesters` | Filters null entries. |
| `getCurrentSemester` | `current-semester` | Dates/ID plus original metadata. |
| `getCourseList` | `courses` | Student/teacher paths, language, time/location enrichment. A failed enrichment is explicitly flagged. |
| Course time/location endpoint | `course-location` | Same endpoint used by course-list enrichment. |
| `getAllContents` | `all-contents` | Sequential aggregation over supplied course IDs; propagates errors instead of silently dropping a course. |
| `getNotificationList` | `notifications` | Current + expired lists, base64 content decode, attachment-page enrichment when present. |
| Notification detail parser | `notification-detail` | HTML and extracted authenticated download links. Viewing may mark notices as read, as in the upstream site. |
| `getFileList` | `files` | File metadata, preview/download URLs, categories. |
| `getFileCategoryList` | `file-categories` | Category IDs/titles/dates. |
| `getFileListByCategory` | `files-by-category` | Handles student array rows and teacher object rows. |
| File download/preview URLs | `file-download`, `file-preview` | Authenticated streaming file download; refuses overwrite. Preview returns HTML, not a graphical PDF viewer. |
| `getHomeworkList` | `homework` | New/submitted/graded status, detail pages and description endpoint; teacher list supported. |
| Homework detail parsers | `homework-detail` | Description, answer, submission HTML and attachment links; preserves full page HTML. |
| Excellent homework helper | `excellent-homework` | Separately exposed instead of silently embedding optional failed requests in every homework list. |
| `submitHomework`; app `submitAssignment` | `homework-submit` | Multipart text/file and deletion flag; requires `--confirm`. |
| Homework download URL | `homework-download` | Authenticated download to caller-chosen new file. |
| `getDiscussionList` | `discussions` | Metadata plus original fields. |
| `getAnsweredQuestionList` | `questions` | Base64 question content, metadata and original fields. Student mode only answered questions. |
| `getQuestionnaireList` | `questionnaires` | Ongoing + ended lists with questions/options. |
| Questionnaire detail helper | `questionnaire-detail` | Questions/options/required flags. |
| `addToFavorites` | `favorite-add` | GET mutation is still guarded by `--confirm`. |
| `removeFromFavorites` | `favorite-remove` | GET mutation is still guarded by `--confirm`. |
| `getFavorites` | `favorites` | Optional course/content type filters. Preserves native result fields. |
| `pinFavoriteItem`, `unpinFavoriteItem` | `favorite-pin`, `favorite-unpin` | Multipart mutation, confirmation required. |
| `setComment` | `comment-set` | Empty content clears annotation; confirmation required. |
| `getComments` | `comments` | Optional course/content type filters, native result fields. |
| `sortCourses` | `courses-sort` | JSON array translated to 1-based ordered course records. |
| `getCalendar` | `calendar` | Learn-to-registrar roaming ticket, strict JSONP parsing (never executes JavaScript), mapped timetable events. |
| `setLanguage` | `language-set` | `zh` / `en`, confirmation required. |
| `getCurrentLanguage` | `language` | Reads server homepage instead of an in-memory mobile-client variable. |
| learnX `uploadCourses` | `coursex-upload` | Native session exchange + GraphQL mutation, explicit caller-owned refresh token, confirmation required. Never uses the app's private build-time token. |

## Important boundaries

1. No real account was supplied. Compilation and offline tests do **not** establish that the school endpoints currently accept these requests, that an account has access, or that enrollment/reservation/submission succeeds.
2. Upstream file/discussion/question endpoints explicitly request `size=200`; this port keeps that documented cap, not an invented pagination protocol. File categories can be enumerated separately. Other page-list endpoints use the same `aoData` protocol as upstream. We do not assert unlimited pagination or full server exports beyond what those source methods expose.
3. Normalized JSON is deliberately not a byte-identical clone of TypeScript return types. Original server fields are preserved under `upstream` where appropriate. Favorites/comments retain native row fields. Grade numeric codes remain available rather than being silently reinterpreted. Excellent homework is a separate operation.
4. Discussion replies, new question posting, questionnaire answer submission and teacher grading are not public callable methods in the inspected `thu-learn-lib`; this module does not invent them. Mobile WebView pages may expose more server functionality than the library port.
5. The mobile app's native calendar permission/UI, widgets, background notifications, file viewer/editor, biometric unlock, OTA/app-update installer and local Redux preference state are not school microservices. They are not reproduced as mobile features in the Linux CLI. Calendar API data can be consumed by the CLI's schedule tools.
6. CourseX is a third-party service at `tsinghua.app`, separate from Tsinghua identity. Its upload command shares course metadata there only after explicit confirmation and requires a caller-owned token. Possession of the school's password does not grant that token.
7. MFA/trusted-device registration and CAPTCHA are not bypassed. Login/session validity depends on the user's later authentication, school network policies and endpoint changes.
8. Downloads reject HTML error/login responses and never overwrite existing files. They do not crawl arbitrary links. Requested preview/detail HTML is data, not instructions, and is not executed.
9. Registrar calendar uses inclusive YYYYMMDD dates, limited to at most 29 days as documented upstream. Fetch multiple adjacent ranges for longer periods.
10. Some reads have server-side effects (notices become read; counters update). They are identified where known. The CLI does not retry a failed state-changing action automatically.

## Examples (replace IDs with values returned by your account)

```sh
thu-cli login --username YOUR_STUDENT_ID --service learn
thu-cli learn semesters
thu-cli learn courses --args '{"semester":"2026-2027-1"}'
thu-cli learn notifications --args '{"course":"COURSE_ID"}'
thu-cli learn files --args '{"course":"COURSE_ID"}'
thu-cli learn file-download --args '{"id":"FILE_ID","output":"lecture.pdf"}'
thu-cli learn homework --args '{"course":"COURSE_ID"}'
thu-cli learn homework-submit --args '{"id":"STUDENT_HOMEWORK_ID","content":"<p>My answer</p>","attachment":"answer.pdf"}' --dry-run
# Check the exact homework and attachment, then submit only when intended:
thu-cli learn homework-submit --args '{"id":"STUDENT_HOMEWORK_ID","content":"<p>My answer</p>","attachment":"answer.pdf"}' --confirm
thu-cli learn calendar --args '{"start":"20261101","end":"20261129"}'
```

For CourseX, create a mode-600 JSON file with `courses` and your own `refresh_token`, review the metadata, then use `--args-file` and `--confirm`. Do not place tokens in shell history, reports or source control.
