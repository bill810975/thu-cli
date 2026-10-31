# THU InfoHelper public API inventory

Source: `thu-info-app/packages/thu-info-lib/src/index.ts`, commit `7930cad3f0d7243ad54df3ce4443b911113eef91`.

Inventory includes **149 explicitly public class members**, including fields and callback hooks, not just network APIs. Each appears once. Classification is based on source inspection, not successful authenticated execution. Callable means a native Rust command exists; it does not mean exact return-model equivalence or live production verification. The original apps expose client adapters over campus systems, not independently deployable backend microservices.

Counts: Local utility / state: 16, Unsupported: 5, Authentication: 8, Callable service: 118, Upstream obsolete / no-op: 2.

| Public member | Classification | CLI mapping | Scope and limits |
|---|---|---|---|
| `userId` | Local utility / state | `—` | Login --username; some service calls require explicit user_id. |
| `password` | Local utility / state | `—` | Hidden login prompt/protected input; no permanent plaintext password storage. |
| `fingerprint` | Local utility / state | `—` | Login --fingerprint. |
| `passkeyAuthenticator` | Unsupported | `—` | Native platform WebAuthn/passkey creation/signing callback is not ported. |
| `passkeyCredential` | Unsupported | `—` | Passkey credential-based login not supported. |
| `passkeyDevice` | Unsupported | `—` | Native passkey device integration not supported. |
| `hasAuthentication` | Local utility / state | `—` | Session/account probes replace in-memory credential boolean. |
| `preparePasskey` | Unsupported | `—` | Passkey enrollment preparation not implemented. |
| `registerPasskey` | Unsupported | `—` | Native passkey enrollment and signing not implemented. |
| `listPasskeys` | Authentication | `thu passkey.list` | No authenticated production test performed. |
| `removePasskey` | Authentication | `thu passkey.remove` | No authenticated production test performed. |
| `renamePasskey` | Authentication | `thu passkey.rename` | No authenticated production test performed. |
| `trustBrowser` | Local utility / state | `—` | Authentication implementation differs; no silent new trusted-device enrollment. |
| `MOCK` | Local utility / state | `—` | Not a production service. Rust fixtures replace magic 8888 account. |
| `mocked` | Local utility / state | `—` | Rust fixture/mock tests, not a production login mode. |
| `graduate` | Local utility / state | `—` | Explicit graduate boolean on academic service calls, not guessed from student-number digits. |
| `clearCookieHandler` | Local utility / state | `—` | Session logout/import facilities replace app callback. |
| `loginErrorHook` | Local utility / state | `—` | CLI stderr/errors replace app callback. |
| `twoFactorMethodHook` | Local utility / state | `—` | CLI authentication interaction. |
| `twoFactorAuthHook` | Local utility / state | `—` | CLI authentication interaction. |
| `twoFactorAuthLimitHook` | Local utility / state | `—` | CLI authentication error/limit handling. |
| `trustFingerprintHook` | Local utility / state | `—` | No automatic trusted-device enrollment. |
| `trustFingerprintNameHook` | Local utility / state | `—` | No automatic trusted-device enrollment. |
| `login` | Authentication | `login (native password flow; passkey login unsupported)` | No authenticated production test performed. |
| `logout` | Authentication | `thu session.logout-remote --confirm` then `logout --confirm` | Separate explicit WebVPN remote logout and local cookie removal; no blanket revocation of every target session. |
| `forgetDevice` | Authentication | `thu device.forget-thu-info` | No authenticated production test performed. |
| `appStartUp` | Local utility / state | `—` | App aggregate, not one microservice. Call app.announcements, app.version, library.records, card.info, course.timetable; telemetry app.startup-stat is explicit. |
| `appUsageStat` | Callable service | `app.usage-stat` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLatestAnnounces` | Callable service | `app.announcements` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLatestVersion` | Callable service | `app.version` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `submitFeedback` | Callable service | `app.feedback` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getFeedbackReplies` | Callable service | `app.feedbacks` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getWeChatGroupQRCodeContent` | Callable service | `app.qrcode` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getPrivacyUrl` | Callable service | `app.privacy` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `switchLang` | Callable service | `info.language` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getUserInfo` | Callable service | `info.user` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getIdAccountInfo` | Callable service | `info.user` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getIdAuthDevices` | Callable service | `info.devices` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getIdLoginLogs` | Callable service | `info.login-logs` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `naiveSendMail` | Callable service | `mail.send` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getReport` | Callable service | `info.report` | Structured grades; source model normalization differs. |
| `getAssessmentList` | Callable service | `assessment.list` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getAssessmentForm` | Callable service | `assessment.form` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `postAssessmentForm` | Callable service | `assessment.submit` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getPhysicalExamResult` | Callable service | `info.physical` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getClassroomList` | Callable service | `classroom.list` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getClassroomState` | Callable service | `classroom.state` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getInvoiceList` | Callable service | `invoice.list` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getInvoicePDF` | Callable service | `invoice.pdf` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getBankPayment` | Callable service | `income.bank` | Explicit years rather than automatic year discovery. |
| `getGraduateIncome` | Callable service | `income.graduate` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCalendar` | Callable service | `calendar.semesters` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCalendarYear` | Callable service | `calendar.year` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCalendarImageUrl` | Callable service | `calendar.image` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCountdown` | Callable service | `info.countdown` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getMadModelToken` | Callable service | `info.model-token` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getDormScore` | Callable service | `dorm.score` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getEleRechargePayCode` | Callable service | `dorm.recharge` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getElePayRecord` | Callable service | `dorm.payments` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getEleRemainder` | Callable service | `dorm.electricity` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `resetDormPassword` | Callable service | `dorm.password` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLibraryList` | Callable service | `library.list` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLibrarySectionList` | Callable service | `library.sections` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLibraryFloorList` | Callable service | `library.floors` | Returns floor tree, does not aggregate every section availability. |
| `getLibrarySeatList` | Callable service | `library.seats` | Returns service seat schema; socket annotations available separately. |
| `toggleSocketState` | Callable service | `library.socket-update` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `bookLibrarySeat` | Callable service | `library.book` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getBookingRecords` | Callable service | `library.records` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `cancelBooking` | Callable service | `library.cancel` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLibraryRoomBookingCaptchaUrl` | Upstream obsolete / no-op | `—` | Upstream resolves an empty string; no active CAPTCHA service. |
| `getLibraryRoomAccNo` | Callable service | `library.room-user` | Fetched account rather than cached property. |
| `loginLibraryRoomBooking` | Authentication | `roam --policy cab / automatic service roaming` | No authenticated production test performed. |
| `getLibraryRoomBookingInfoList` | Callable service | `library.room-list` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLibraryRoomBookingResourceList` | Callable service | `library.room-resources` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `fuzzySearchLibraryId` | Callable service | `library.room-members` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `bookLibraryRoom` | Callable service | `library.room-book` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getLibraryRoomBookingRecord` | Callable service | `library.room-records` | Explicit date range instead of default next six days. |
| `cancelLibraryRoomBooking` | Callable service | `library.room-cancel` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `updateLibraryRoomEmail` | Callable service | `library.room-email` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNewsList` | Callable service | `news.list` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchNewsList` | Callable service | `news.search` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNewsSubscriptionList` | Callable service | `news.subscriptions` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNewsSourceList` | Callable service | `news.sources` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNewsChannelList` | Callable service | `news.channels` | Returns both source-language fields, not needEnglish transformation. |
| `addNewsSubscription` | Callable service | `news.subscription-add` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `removeNewsSubscription` | Callable service | `news.subscription-remove` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNewsListBySubscription` | Callable service | `news.subscribed` | Empty subscription ID requests all subscriptions; check describe for current arguments. |
| `getNewsDetail` | Callable service | `news.detail` | ID detail endpoint and URL-based legacy/PDF handling; structured content differs from upstream HTML view model. |
| `addNewsToFavor` | Callable service | `news.favorite-add` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `removeNewsFromFavor` | Callable service | `news.favorite-remove` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getFavorNewsList` | Callable service | `news.favorites` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getSchedule` | Callable service | `schedule.list` | Date-range CLI, explicit first_day for secondary; summer CR events supported via semester/first_day context. Missing semester context marks output incomplete. |
| `saveCustomSchedule` | Callable service | `schedule.create` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `deleteCustomSchedule` | Callable service | `schedule.delete` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCrTimetable` | Callable service | `course.timetable` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCrCaptchaUrl` | Callable service | `course.captcha` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `loginCr` | Upstream obsolete / no-op | `—` | Upstream explicitly throws obsolete-method error; use CR SSO roaming. |
| `getCrAvailableSemesters` | Callable service | `course.semesters` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCrCoursePlan` | Callable service | `course.plan` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchCrRemaining` | Callable service | `course.remaining` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchCrPrimaryOpen` | Callable service | `course.open` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchCrCourses` | Callable service | `course.search` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `selectCourse` | Callable service | `course.select` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `deleteCourse` | Callable service | `course.delete` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getSelectedCourses` | Callable service | `course.selected` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `changeCourseWill` | Callable service | `course.will` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCrCurrentStage` | Callable service | `course.stage` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchCoursePriorityMeta` | Callable service | `course.priority-meta` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchCoursePriorityInformation` | Callable service | `course.priority` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getQueueInfo` | Callable service | `course.queue` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `cancelCoursePF` | Callable service | `course.pf-cancel` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `setCoursePF` | Callable service | `course.pf-set` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getSportsResources` | Callable service | `sports.resources` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `updateSportsPhoneNumber` | Callable service | `sports.phone-update` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getSportsCaptchaUrl` | Callable service | `sports.captcha` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `makeSportsReservation` | Callable service | `sports.book + sports.payment-qr` | Order creation and QR payment initialization are separate explicit actions. |
| `getSportsReservationRecords` | Callable service | `sports.records` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `paySportsReservation` | Callable service | `sports.payment-qr` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `unsubscribeSportsReservation` | Callable service | `sports.cancel` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitNamespaces` | Callable service | `gitlab.namespaces` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitRecentProjects` | Callable service | `gitlab.recent` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitOwnedProjects` | Callable service | `gitlab.personal` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `searchGitProjects` | Callable service | `gitlab.search` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitStarredProjects` | Callable service | `gitlab.starred` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitProjectDetail` | Callable service | `gitlab.project` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitProjectTree` | Callable service | `gitlab.tree` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitProjectBranches` | Callable service | `gitlab.branches` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getGitProjectFileBlob` | Callable service | `gitlab.blob` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `renderGitMarkdown` | Callable service | `gitlab.markdown` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getDegreeProgramCompletion` | Callable service | `program.completion` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getFullDegreeProgram` | Callable service | `program.detail` | program.list + program.detail expose source tables rather than upstream recursive degree aggregation. |
| `getOnlineDevices` | Callable service | `network.devices` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNetworkBalance` | Callable service | `network.balance` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getNetworkAccountInfo` | Callable service | `network.account` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `logoutNetworkDevice` | Callable service | `network.logout` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `loginNetworkDevice` | Callable service | `network.login` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getScoreByCourseId` | Callable service | `thos.score` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `prepareThosSession` | Callable service | `thos.counts` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getThosTasks` | Callable service | `thos.tasks` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getThosServices` | Callable service | `thos.services` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `loginCampusCard` | Authentication | `roam --policy card / automatic service roaming` | No authenticated production test performed. |
| `getCampusCardInfo` | Callable service | `card.info` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCampusCardPhotoUrl` | Callable service | `card.photo` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `getCampusCardTransactions` | Callable service | `card.transactions` | Paged instead of fixed 10000-row request. |
| `changeCampusCardPassword` | Callable service | `card.password-change` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `modifyCampusCardMaxTransactionAmount` | Callable service | `card.limits` | Exact cents/server field names prevent upstream daily/once mapping ambiguity. |
| `reportCampusCardLoss` | Callable service | `card.report-loss` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `cancelCampusCardLoss` | Callable service | `card.cancel-loss` | Native endpoint/protocol port; output schema may differ; live behavior unverified. |
| `rechargeCampusCard` | Callable service | `card.recharge-bank / card.recharge-qr` | Compatibility check then explicit bank transfer or QR handoff; no QR payment completion. |

Consult `thu-coverage.md`, `reservations.md`, `card.md` and `thu-cli describe` for exact argument schemas and remaining differences. Source helper methods not exported on InfoHelper may also have additional CLI commands (e.g. individual dates/segments and QR handoff). No count in this document establishes full service equivalence.
