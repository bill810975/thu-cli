// Native Rust port of thu-info-lib protocols; see THIRD_PARTY_NOTICES.
#![allow(dead_code)]
use crate::session::Session;
use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::{json, Value};
const CONTENT_TYPE_FORM: &str = "application/x-www-form-urlencoded";
const CONTENT_TYPE_JSON: &str = "application/json";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.88 Safari/537.36";
const WEB_VPN_ROOT_URL: &str = "https://webvpn.tsinghua.edu.cn";
const LOGIN_URL: &str = "https://webvpn.tsinghua.edu.cn/login";
const WEB_VPN_OAUTH_LOGIN_URL: &str = "https://webvpn.tsinghua.edu.cn/login?oauth_login=true";
const ID_HOST_URL: &str = "https://id.tsinghua.edu.cn";
const ID_BASE_URL: &str = "https://id.tsinghua.edu.cn/do/off/ui/auth/login/form/";
const ID_LOGIN_URL: &str = "https://id.tsinghua.edu.cn/do/off/ui/auth/login/check";
const DOUBLE_AUTH_URL: &str = "https://id.tsinghua.edu.cn/b/doubleAuth/login";
const SAVE_FINGER_URL: &str = "https://id.tsinghua.edu.cn/b/doubleAuth/personal/saveFinger";
const ID_WEBSITE_BASE_URL: &str = "https://id.tsinghua.edu.cn/f/login";
const ID_WEBSITE_LOGIN_URL: &str = "https://id.tsinghua.edu.cn/security_check";
const ID_ACCOUNT_SETTINGS_URL: &str = "https://id.tsinghua.edu.cn/f/account/settings";
const ID_LOGIN_LOG_DATA_URL: &str = "https://id.tsinghua.edu.cn/f/account/loginLog/data";
const CHECK_CURRENT_DEVICE_URL: &str = "https://id.tsinghua.edu.cn/b/account/checkCurrentDeviceIfNeedAdd?currentFinger={fingerprint}&device_gen_finger_print=null";
const GET_DEVICE_LIST_URL: &str = "https://id.tsinghua.edu.cn/b/account/getDeviceList";
const DELETE_DEVICE_URL: &str = "https://id.tsinghua.edu.cn/b/account/deleteDevice";
const WEB_VPN_ID_BASE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f9f30f8834396657761d88e29d51367bcfe7/do/off/ui/auth/login/form/";
const WEB_VPN_ID_LOGIN_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f9f30f8834396657761d88e29d51367bcfe7/do/off/ui/auth/login/check";
const GET_COOKIE_URL: &str = "https://webvpn.tsinghua.edu.cn/wengine-vpn/cookie?method=get&host=info.tsinghua.edu.cn&scheme=https&path=/f/info/gxfw_fg/common/index";
const USER_DATA_URL: &str = "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/grjbxx";
const ROAMING_URL: &str =
    "https://info.tsinghua.edu.cn/b/yyfw/vyyfwxx/info/portal_fg/common/onlineAppRedirect";
const LOGOUT_URL: &str = "https://webvpn.tsinghua.edu.cn/logout";
const SWITCH_LANG_URL: &str = "https://info.tsinghua.edu.cn/f/lang/change?websiteShowLanguage=";
const CALENDAR_URL: &str = "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/xl";
const DEADLINE_URL: &str = "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/deadline/list";
const CALENDAR_IMAGE_URL: &str = "https://app.cs.tsinghua.edu.cn/xiaoli";
const CALENDAR_YEAR_URL: &str = "https://app.cs.tsinghua.edu.cn/Api/SchoolCalendarYear";
const LEARN_HOME_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fcf2408e297e7c4377068ea48d546d30ca8cc97bcc/f/wlxt/index/course/student/index";
const SEMESTER_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fcf2408e297e7c4377068ea48d546d30ca8cc97bcc/b/kc/zhjw_v_code_xnxq/getCurrentAndNextSemester?_csrf=";
const EMAIL_BASE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fdf64890347e7c4377068ea48d546d3011ff591d40/coremail/XT3/";
const COUNT_DOWN_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/portal3rd.do?url=/portal3rd.do&m=tsxx";
const DORM_LOGIN_URL_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fdb94c852f3f6555301c9aa596522b20e7a45e0b22fda391/weixin/weixin_user_authenticate.aspx";
const DORM_SCORE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fdb94c852f3f6555301c9aa596522b20e7a45e0b22fda391/weixin/weixin_health_linechart.aspx?id=0";
const GET_BKS_REPORT_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/cj.cjCjbAll.do?m=bks_cjdcx&cjdlx=zw";
const GET_YJS_REPORT_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/cj.cjCjbAll.do?m=yjs_cjdcx&cjdlx=zw";
const BKS_REPORT_BXR_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/cj.cjCjbAll.do?m=bks_yxkccj";
const YJS_REPORT_BXR_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/cj.cjCjbAll.do?m=yjs_yxkccj";
const RECHARGE_ELE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421fdee49932a3526446d0187ab9040227bca90a6e14cc9/netweb_user/recharge_ele.aspx";
const RECHARGE_PAY_ELE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421fdee49932a3526446d0187ab9040227bca90a6e14cc9/netweb_user/recharge_pay_ele.aspx";
const ELE_PAY_RECORD_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421fdee49932a3526446d0187ab9040227bca90a6e14cc9/Netweb_List/netweb_ele_pay_record.aspx";
const ELE_REMAINDER_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421fdee49932a3526446d0187ab9040227bca90a6e14cc9/Netweb_List/Netweb_Home_electricity_Detail.aspx";
const CHANGE_HOME_PASSWORD_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421fdee49932a3526446d0187ab9040227bca90a6e14cc9/Netweb_List/ChangePassword.aspx";
const ASSESSMENT_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421faef469069336153301c9aa596522b20e33c1eb39606919f/jxpg/f/jxpg/wj/xs/pgkcList";
const ASSESSMENT_BASE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421faef469069336153301c9aa596522b20e33c1eb39606919f";
const ASSESSMENT_SUBMIT_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421faef469069336153301c9aa596522b20e33c1eb39606919f/jxpg/b/jxpg/pgjg/xs/zancunjs";
const EXPENDITURE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f5f4408e237e7c4377068ea48d546d303341e9882a/user/ExDetailsDown.do";
const JXRL_BKS_URL: &str =
    "https://info.tsinghua.edu.cn/f/info/gxfw_fg/common/cardRoaming/JXRL_BKS";
const JXRL_YJS_URL: &str =
    "https://info.tsinghua.edu.cn/f/info/gxfw_fg/common/cardRoaming/JXRL_YJS";
const JXMH_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/jxmh.do";
const JXRL_BKS_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/jxmh_out.do?m=bks_jxrl_all&p_start_date=";
const JXRL_YJS_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/jxmh_out.do?m=yjs_jxrl_all&p_start_date=";
const JXRL_MIDDLE: &str = "&p_end_date=";
const JXRL_SUFFIX: &str = "&jsoncallback=m";
const SECONDARY_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/portal3rd.do?m=bks_ejkbSearch";
const PDF_NEWS_PREFIX: &str = "https://info.tsinghua.edu.cn/b/info/wj/downloadPdfStream/";
const SYSC_PDF_NEWS_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f5468534367f1e6d119aafd641303ceb8f9190006d6afc78336870/tsinghua.war/yct.www.api/publish/";
const NEWS_LIST_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/xxfb_fg/xnzx/template/more?oType=xs&lydw=";
const SEARCH_NEWS_LIST_URL: &str =
    "https://info.tsinghua.edu.cn/b/xnzx/search/info/xxfb_fg/teacher/getMobilePageList";
const NEWS_REDIRECT_URL: &str = "https://info.tsinghua.edu.cn";
const NEWS_DETAIL_URL: &str = "https://info.tsinghua.edu.cn/b/info/xxfb_fg/xnzx/template/detail";
const NEWS_ADD_FAVOR_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/addFavorite/XXFB/";
const NEWS_REMOVE_FAVOR_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/delFavorite/XXFB/";
const NEWS_FAVOR_LIST_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/queryFavoriteXxfbPageList";
const NEWS_SUBSCRIPTION_LIST_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/querySubscribeConditionNameList/XXFB";
const NEWS_SOURCE_LIST_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/querySubscribeInformationUnitList";
const NEWS_CHANNEL_LIST_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/xxfb_fg/teacher/lm/subscribe/getlmListByDwh";
const NEWS_ADD_SUBSCRIPTION_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/addSubscribeCondition";
const NEWS_REMOVE_SUBSCRIPTION_URL_FORMAT: &str = "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/deleteSubscribeCondition/{id}/XXFB?_csrf={csrf}";
const NEWS_LIST_BY_SUBSCRIPTION_URL: &str =
    "https://info.tsinghua.edu.cn/b/info/gxfw_fg/common/querySubscribeInfomationPageList";
const FILE_DOWNLOAD_URL: &str = "https://info.tsinghua.edu.cn/b/info/wj/download/";
const CLASSROOM_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/portal3rd.do?url=/portal3rd.do&m=jasJy_Xs_Js_index";
const CLASSROOM_STATE_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/pk.classroomctrl.do?m=qyClassroomState&classroom=";
const CLASSROOM_STATE_MIDDLE: &str = "&weeknumber=";
const INVOICE_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f4ed519669247b59700f81b9991b2631aee63c51/invoiceSys/getList.do";
const INVOICE_CONTENT_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f4ed519669247b59700f81b9991b2631aee63c51/invoice/showInvPdf.do?uuid=";
const INVOICE_LOGIN_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f4ed519669247b59700f81b9991b2631aee63c51/roam/roamAuth.do";
const LOSE_CARD_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f5f4408e237e7c4377068ea48d546d303341e9882a/user/RambleConsumeLog.do?losscard=true";
const BANK_PAYMENT_SEARCH_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421e9ff459a69247b59700f81b9991b26317dbd36ae/yhdfcx/search.do";
const FOUNDATION_BANK_PAYMENT_SEARCH_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421e9ff459a69247b59700f81b9991b26317dbd36ae/yhdfcx_jjh/search.do";
const GRADUATE_INCOME_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaed4b9069377a517a1d88b89d1b37269c624d2b1c6925f37faea82b8d/b/yjsjzxt/v_yjszzjl_yjscwdfmx_cx/pageList";
const LIBRARY_HOME_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/home/web/f_second";
const LIBRARY_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/api.php/areas/1/tree/1";
const LIBRARY_AREAS_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/api.php/areas/";
const LIBRARY_SEATS_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/api.php/spaces_old";
const LIBRARY_DAYS_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/api.php/areadays/";
const LIBRARY_IMAGE_BASE: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/Public/home/images/web/area/";
const LIBRARY_ROOM_BOOKING_ROOT_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1";
const LIBRARY_ROOM_BOOKING_QUERY_AUTH_ADDRESS_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/auth/address?finalAddress=https:%2F%2Fcab.lib.tsinghua.edu.cn&errPageUrl=https:%2F%2Fcab.lib.tsinghua.edu.cn%2F%23%2Ferror&manager=false&consoleType=16";
const LIBRARY_ROOM_BOOKING_USER_INFO_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/auth/userInfo";
const LIBRARY_ROOM_BOOKING_ROOM_INFO_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/roomDevice/roomInfos";
const LIBRARY_ROOM_BOOKING_RESOURCE_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/reserve?sysKind=1";
const LIBRARY_FUZZY_SEARCH_ID_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/account/getMembers?page=1&pageNum=10&key=";
const LIBRARY_ROOM_BOOKING_ACTION_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/reserve";
const LIBRARY_ROOM_BOOKING_RECORD_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/reserve/resvInfo?needStatus=8454&orderKey=gmt_create&orderModel=desc";
const LIBRARY_CANCEL_BOOKING_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/reserve/delete";
const LIBRARY_ROOM_UPDATE_EMAIL_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f3f643d22b396a1e6a1b80a29f5d363409e413829737d1/ic-web/account/update";
const LIBRARY_BOOK_URL_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/api.php/spaces/";
const LIBRARY_BOOK_URL_SUFFIX: &str = "/book";
const LIBRARY_BOOK_RECORD_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/user/index/book";
const CANCEL_BOOKING_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e3f24088693c6152301c9aa596522b204c02212b859d0a19/api.php/profile/books/";
const PHYSICAL_EXAM_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/tyjx.tyjx_tc_xscjb.do?m=jsonCj";
const CR_TIMETABLE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f1f94f93323e6b55300b80afd641303c054d0551650ca167136885bf/sites/default/files/%E9%80%89%E8%AF%BE%E6%97%B6%E9%97%B4%E5%AE%89%E6%8E%92.htm";
const CR_CAPTCHA_URL: &str =
    "https://zhjwxk.cic.tsinghua.edu.cn/login-jcaptcah.jpg?vpn-1&captchaflag=login1";
const ACADEMIC_LOGIN_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f1f44098223d6153301c9aa596522b2027124ec39c5debe6/Login";
const ACADEMIC_ROOT_URL: &str = "https://webvpn.tsinghua.edu.cn/http-80/77726476706e69737468656265737421f1f44098223d6153301c9aa596522b2027124ec39c5debe6/render.userLayoutRootNode.uP";
const KCXX_BKS_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f9f9479369247b59700f81b9991b2631506205de/f/info/gxfw_fg/common/cardRoaming/KCXX_BKS";
const KCXX_YJS_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421f9f9479369247b59700f81b9991b2631506205de/f/info/gxfw_fg/common/cardRoaming/KCXX_YJS";
const CR_LOGIN_HOME_URL: &str =
    "https://zhjwxk.cic.tsinghua.edu.cn/xklogin.do?UIDisplayLanguage=zh_CN";
const CR_SEARCH_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkBks.vxkBksJxjhBs.do";
const CR_ZYTJB_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkBks.xkBksZytjb.do";
const COURSE_PLAN_URL_PREFIX: &str = "https://zhjwxk.cic.tsinghua.edu.cn/jhBks.vjhBksPyfakcbBs.do?m=showBksZxZdxjxjhXmxqkclist&pathContent=%B1%BE%D1%A7%C6%DA%BD%CC%D1%A7%BC%C6%BB%AE&p_xnxq=";
const CR_MAIN_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkBks.vxkBksXkbBs.do?m=main";
const CR_TREE_URL: &str =
    "https://zhjwxk.cic.tsinghua.edu.cn/xkBks.vxkBksXkbBs.do?m=showTree&p_xnxq=";
const CR_SELECT_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkBks.vxkBksXkbBs.do";
const CR_COURSE_DETAIL_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/js.vjsKcbBs.do";
const CR_WHOLE_TIMETABLE_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/syxk.vsyxkKcapb.do";
const CR_SEARCH_YJS_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkYjs.vxkYjsJxjhBs.do";
const CR_ZYTJB_YJS_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkYjs.xkYjsZytjb.do";
const COURSE_PLAN_YJS_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/jhYjs.vjhYjsGrpyfab.do";
const CR_MAIN_YJS_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkYjs.vxkYjsXkbBs.do?m=main";
const CR_TREE_YJS_URL: &str =
    "https://zhjwxk.cic.tsinghua.edu.cn/xkYjs.vxkYjsXkbBs.do?m=showTree&p_xnxq=";
const CR_SELECT_YJS_URL: &str = "https://zhjwxk.cic.tsinghua.edu.cn/xkYjs.vxkYjsXkbBs.do";
const SPORTS_BASE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/gymbook/gymBookAction.do?ms=viewGymBook&viewType=m";
const SPORTS_DETAIL_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/gymsite/cacheAction.do?ms=viewBook&userType=1";
const SPORTS_UNPAID_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/pay/payAction.do?ms=getOrdersForNopay";
const SPORTS_PAID_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/pay/payAction.do?ms=getOrdersForUnpay";
const SPORTS_UNSUBSCRIBE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/gymbook/gymBookAction.do?ms=unsubscribe";
const SPORTS_QUERY_PHONE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/gymbook/gymBookAction.do?ms=hadContactOrNot";
const SPORTS_UPDATE_PHONE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/gymbook/gymBookAction.do?ms=doUpdateContactInformation&cell_phone=";
const SPORTS_CAPTCHA_BASE_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/Kaptcha.jpg";
const SPORTS_MAKE_ORDER_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/gymbook/gymbook/gymBookAction.do?vpn-12-o1-50.tsinghua.edu.cn=&ms=saveGymBook";
const SPORTS_MAKE_PAYMENT_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/pay/payAction.do?ms=newPay";
const SPORTS_MAKE_PAYMENT_LATER_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421a5a70f8834396657761d88e29d51367b6a00/pay/payAction.do?ms=newPayForLater";
const SPORTS_PAYMENT_CHECK_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f6f60c93293c615e7b469dbf915b243daf0f96e17deaf447b4/zjjsfw/zjjs/check.do";
const SPORTS_PAYMENT_ACTION_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421f6f60c93293c615e7b469dbf915b243daf0f96e17deaf447b4/zjjsfw/zjjs/webPay.do";
const GITLAB_LOGIN_URL: &str = "https://git.tsinghua.edu.cn/users/sign_in";
const GITLAB_AUTH_URL: &str = "https://git.tsinghua.edu.cn/users/auth/thuid";
const GITLAB_API_BASE_URL: &str = "https://git.tsinghua.edu.cn/api/v4";
const PROGRAM_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/jhBks.by_fascjgmxb_gr.do?m=queryFaScjgmx_gr&xsViewFlag=pyfa&pathContent=%E5%9F%B9%E5%85%BB%E6%96%B9%E6%A1%88%E5%AE%8C%E6%88%90%E6%83%85%E5%86%B5";
const PROGRAM_LIST_URL: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/jhBks.vjhBksPyfabBs.do?m=grPyfabks&theRole=&theModule=pyfa&pathContent=%E4%B8%AA%E4%BA%BA%E5%9F%B9%E5%85%BB%E6%96%B9%E6%A1%88";
const FULL_PROGRAM_URL_PREFIX: &str = "https://webvpn.tsinghua.edu.cn/http/77726476706e69737468656265737421eaff4b8b69336153301c9aa596522b20bc86e6e559a9b290/jhBks.vjhBksPyfakcbBs.do?m=index2&theModule=pyfa&p_fajhh=";
const APP_ANNOUNCEMENT_URL: &str = "https://app.cs.tsinghua.edu.cn/api/announce?page=1&version=";
const APP_LATEST_VERSION_URL: &str = "https://app.cs.tsinghua.edu.cn/api/version";
const APP_STARTUP_STAT_URL: &str = "https://app.cs.tsinghua.edu.cn/stat/startup";
const APP_USAGE_STAT_URL: &str = "https://app.cs.tsinghua.edu.cn/stat/usage";
const APP_SUBMIT_FEEDBACK_URL: &str = "https://app.cs.tsinghua.edu.cn/api/feedback";
const APP_FEEDBACKS_URL: &str = "https://app.cs.tsinghua.edu.cn/api/repliedfeedback";
const APP_QRCODE_URL: &str = "https://app.cs.tsinghua.edu.cn/api/qrcode";
const APP_SOCKET_STATUS_URL: &str = "https://app.cs.tsinghua.edu.cn/api/socket";
const APP_PRIVACY_URL: &str = "https://app.cs.tsinghua.edu.cn/privacy";
const NETWORK_LOGIN_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73/login";
const NETWORK_HOME_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73/home";
const NETWORK_HOME_DELETE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73/home/delete?id={id}&user_mac={mac}";
const NETWORK_IMPORT_DEVICE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73/certification";
const NETWORK_USER_INFO_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73/users";
const NETWORK_ALLOWED_DEVICES_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e5e4448e223726446d0187ab9040227b54b6c80fcd73/user/online-num";
const MADMODEL_BASE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fdf6459128346d5c300b9ae28c462a3b27469fc32211fa26a3e464";
const THOS_SELECT_ONE_URL: &str = "https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e4ff4e8f69247b59700f81b9991b2631ca359dd4/fp/fp/Uniformcommon/selectOnePresetData";
const CARD_I_VERSION_URL: &str = "https://app.cs.tsinghua.edu.cn/api/CardIVersion";
const CARD_USER_BY_TOKEN_URL: &str = "https://card.tsinghua.edu.cn/login/getUserInfoFromToken";
const CARD_INFO_BY_USER_URL: &str = "https://card.tsinghua.edu.cn/business/getCardUserinfo";
const CARD_PHOTO_URL: &str = "https://card.tsinghua.edu.cn/myaccount/showDbImage?idserial=";
const CARD_TRANSACTION_URL: &str = "https://card.tsinghua.edu.cn/business/querySelfTradeList";
const CARD_CHANGE_PWD_URL: &str = "https://card.tsinghua.edu.cn/business/modifyPwdByPhoneVerify";
const CARD_MOD_MAX_CONSUME_URL: &str = "https://card.tsinghua.edu.cn/business/modifyCardMaxConsamt";
const CARD_REPORT_LOSS_URL: &str = "https://card.tsinghua.edu.cn/business/cardReportLoss";
const CARD_CANCEL_LOSS_URL: &str = "https://card.tsinghua.edu.cn/business/solutionHang";
const CARD_RECHARGE_FROM_BANK_URL: &str = "https://card.tsinghua.edu.cn/business/moblieRecharge";
const CARD_RECHARGE_FROM_WECHAT_URL: &str =
    "https://card.tsinghua.edu.cn/wx/rechard/getRechargeHtml";
const CARD_RECHARGE_FROM_ALIPAY_URL: &str = "https://card.tsinghua.edu.cn/wx/rechard/qrcode";
fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static CSS selector")
}
fn text(e: scraper::ElementRef<'_>) -> String {
    e.text().collect::<Vec<_>>().join("").trim().to_owned()
}
fn cells(e: scraper::ElementRef<'_>) -> Vec<String> {
    e.select(&sel("td")).map(text).collect()
}
fn tables(html: &str, selector: &str) -> Value {
    json!(Html::parse_document(html)
        .select(&sel(selector))
        .map(cells)
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>())
}
fn s(a: &Value, k: &str) -> Result<String> {
    match &a[k] {
        Value::String(v) => Ok(v.clone()),
        Value::Number(v) => Ok(v.to_string()),
        _ => bail!("Missing string/number argument: {k}"),
    }
}
fn d(a: &Value, k: &str, v: &str) -> String {
    s(a, k).unwrap_or_else(|_| v.to_owned())
}
fn flag(a: &Value, k: &str) -> bool {
    a[k].as_bool().unwrap_or(false)
}
fn form(v: &Value) -> Result<Vec<(String, String)>> {
    Ok(v.as_object()
        .context("Expected object")?
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                match v {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    _ => v.to_string(),
                },
            )
        })
        .collect())
}
fn encoded(v: &str, gbk: bool) -> String {
    let bytes = if gbk {
        encoding_rs::GBK.encode(v).0.into_owned()
    } else {
        v.as_bytes().to_vec()
    };
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
async fn body(r: reqwest::Response) -> Result<String> {
    let r = r.error_for_status()?;
    let ct = r
        .headers()
        .get("content-type")
        .and_then(|x| x.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let b = r.bytes().await?;
    let text = if ct.contains("gb") {
        encoding_rs::GBK.decode(&b).0.into_owned()
    } else {
        match String::from_utf8(b.to_vec()) {
            Ok(s) => s,
            Err(_) => encoding_rs::GBK.decode(&b).0.into_owned(),
        }
    };
    if text.contains("needCaptcha")
        || text.contains("用户登陆超时或访问内容不存在")
        || text.contains("<title>清华大学WebVPN</title>")
        || text.contains("do/off/ui/auth/login")
        || text.contains("sm2publicKey")
    {
        bail!("Session expired or captcha required; refresh login/session before retrying");
    }
    Ok(text)
}
async fn get(c: &Client, u: &str) -> Result<String> {
    body(c.get(u).send().await?).await
}
async fn post(c: &Client, u: &str, v: &Value, gbk: bool) -> Result<String> {
    let f = form(v)?;
    let b = f
        .iter()
        .map(|(k, v)| format!("{}={}", encoded(k, gbk), encoded(v, gbk)))
        .collect::<Vec<_>>()
        .join("&");
    body(
        c.post(u)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(b)
            .send()
            .await?,
    )
    .await
}
async fn jget(c: &Client, u: &str) -> Result<Value> {
    serde_json::from_str(&get(c, u).await?)
        .context("Expected JSON response, received login page or changed API")
}
async fn jpost(c: &Client, u: &str, v: &Value) -> Result<Value> {
    serde_json::from_str(&body(c.post(u).json(v).send().await?).await?)
        .context("Expected JSON response")
}
fn hidden(html: &str, name: &str) -> Result<String> {
    Html::parse_document(html)
        .select(&sel("input"))
        .find(|e| e.value().attr("name") == Some(name))
        .and_then(|e| e.value().attr("value"))
        .map(str::to_owned)
        .with_context(|| format!("Missing {name} input; authentication or page format changed"))
}
fn hidden_all(html: &str) -> Value {
    let mut m = serde_json::Map::new();
    for e in Html::parse_document(html).select(&sel("input[type=hidden]")) {
        if let Some(n) = e.value().attr("name") {
            m.insert(n.to_owned(), json!(e.value().attr("value").unwrap_or("")));
        }
    }
    Value::Object(m)
}
fn capture(html: &str, re: &str) -> Result<String> {
    Regex::new(re)?
        .captures(html)
        .and_then(|x| x.get(1))
        .map(|x| x.as_str().to_owned())
        .context("Response format changed; expected field absent")
}
async fn csrf(session: &Session) -> Result<String> {
    if let Some(token) = session.cookie_value("info.tsinghua.edu.cn", "XSRF-TOKEN")? {
        return Ok(token);
    }
    let cookies = get(&session.client, GET_COOKIE_URL).await?;
    capture(&(cookies + ";"), r"XSRF-TOKEN=(.+?);")
        .context("information-portal CSRF cookie missing; session refresh required")
}
fn crurl(a: &Value, bks: &'static str, yjs: &'static str) -> &'static str {
    if flag(a, "graduate") {
        yjs
    } else {
        bks
    }
}
fn query(u: &str, p: &Value) -> Result<String> {
    let mut u = reqwest::Url::parse(u)?;
    for (k, v) in form(p)? {
        u.query_pairs_mut().append_pair(&k, &v);
    }
    Ok(u.to_string())
}
fn message(html: &str) -> Result<Value> {
    Ok(json!({"message":capture(html,r#"showMsg\("(.+?)"\);"#)?}))
}
fn field_value(row: &[String], n: usize) -> Value {
    row.get(n).map(|x| json!(x)).unwrap_or(Value::Null)
}
fn normalized_sequence(value: &Value) -> String {
    let text = match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    };
    let normalized = text.trim_start_matches('0').to_owned();
    if normalized.is_empty() {
        "0".to_owned()
    } else {
        normalized
    }
}
fn named_rows(html: &str, selector: &str, fields: &[(usize, &str)]) -> Value {
    json!(Html::parse_document(html)
        .select(&sel(selector))
        .map(|e| {
            let t = cells(e);
            let mut m = serde_json::Map::new();
            for (n, k) in fields {
                m.insert(k.to_string(), field_value(&t, *n));
            }
            Value::Object(m)
        })
        .collect::<Vec<_>>())
}

fn course_catalog_rows(html: &str) -> Value {
    let teacher_re = Regex::new(r"p_jsh=([^&]+)").expect("static p_jsh regex");
    json!(Html::parse_document(html)
        .select(&sel("tr.trr2"))
        .map(|row| {
            let t = cells(row);
            let teacher_href = row
                .select(&sel("a[href*='showJsDetail']"))
                .next()
                .and_then(|link| link.value().attr("href"))
                .unwrap_or("");
            let detail_url = row
                .select(&sel("a[href*='showToXs']"))
                .next()
                .and_then(|link| link.value().attr("href"))
                .unwrap_or("");
            let teacher_id = teacher_re
                .captures(teacher_href)
                .and_then(|capture| capture.get(1))
                .map(|capture| capture.as_str())
                .unwrap_or("");
            json!({
                "department": field_value(&t, 0),
                "id": field_value(&t, 1),
                "seq": field_value(&t, 2),
                "name": field_value(&t, 3),
                "credits": field_value(&t, 4),
                "teacher": field_value(&t, 5),
                "teacherId": teacher_id,
                "bksCapacity": field_value(&t, 6),
                "bksRemaining": field_value(&t, 7),
                "yjsCapacity": field_value(&t, 8),
                "yjsRemaining": field_value(&t, 9),
                "time": field_value(&t, 10),
                "note": field_value(&t, 11),
                "feature": field_value(&t, 12),
                "year": field_value(&t, 13),
                "secondary": field_value(&t, 14),
                "reUseCap": field_value(&t, 16),
                "restrict": field_value(&t, 17),
                "culture": field_value(&t, 18),
                "detailUrl": detail_url
            })
        })
        .collect::<Vec<_>>())
}

fn course_level_rows(html: &str) -> Value {
    let code_re = Regex::new(r"^[A-Za-z0-9]+$").expect("static course code regex");
    json!(Html::parse_document(html)
        .select(&sel("tr.trr2"))
        .filter_map(|row| {
            let values = cells(row);
            let code_index = values.iter().position(|value| {
                value.len() >= 6
                    && value.chars().any(|character| character.is_ascii_digit())
                    && code_re.is_match(value)
            })?;
            let id = values.get(code_index)?;
            let seq = values
                .get(code_index + 1)
                .map(|value| value.trim_start_matches('0'))
                .filter(|value| !value.is_empty())
                .unwrap_or("0");
            let property = values.get(code_index + 2).cloned().unwrap_or_default();
            let (type_code, type_label) = match property.as_str() {
                "必修" => ("006", "必修"),
                "限选" => ("008", "限选"),
                "任选" => ("007", "任选"),
                _ => ("ty", "体育"),
            };
            Some(json!({
                "id": id,
                "seq": seq,
                "property": property,
                "typeCode": type_code,
                "typeLabel": type_label
            }))
        })
        .collect::<Vec<_>>())
}

fn selected_course_rows(html: &str) -> Result<Value> {
    use std::collections::BTreeMap;

    let allocation_re = Regex::new(
        r#"(?s)\[\s*"(\d+),(\d+)"\s*,\s*"(\d+)"\s*,\s*"(\d+)"\s*,\s*"([^"]*)"\s*,\s*"[^"]*"\s*\]"#,
    )?;
    let mut allocations = BTreeMap::new();
    for capture in allocation_re.captures_iter(html) {
        let code = capture.get(1).map(|value| value.as_str()).unwrap_or("");
        let seq = capture.get(2).map(|value| value.as_str()).unwrap_or("");
        let will = capture.get(3).map(|value| value.as_str()).unwrap_or("");
        let type_code = capture.get(4).map(|value| value.as_str()).unwrap_or("");
        let sports = capture.get(5).map(|value| value.as_str()).unwrap_or("");
        allocations.insert(
            format!("{code}_{seq}"),
            (will.to_owned(), type_code.to_owned(), sports.to_owned()),
        );
    }

    let code_re = Regex::new(r"^[A-Za-z0-9]+$")?;
    let mut rows = Vec::new();
    for row in Html::parse_document(html).select(&sel("tr.trr2")) {
        let delete_id = row
            .select(&sel("input[name='p_del_id']"))
            .find_map(|input| input.value().attr("value"))
            .unwrap_or("");
        let id_parts = delete_id.split(';').collect::<Vec<_>>();
        let values = cells(row);
        let mut id = id_parts.get(1).copied().unwrap_or("").to_owned();
        let mut seq = id_parts.get(2).copied().unwrap_or("").to_owned();
        let code_index = values.iter().position(|value| {
            value.len() >= 6
                && value.chars().any(|character| character.is_ascii_digit())
                && code_re.is_match(value)
        });
        let current_layout = code_index.is_some_and(|index| {
            values.get(index + 1).is_some_and(|value| {
                !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
            })
        });
        if let Some(index) = code_index {
            id = values[index].clone();
            seq = values.get(index + 1).cloned().unwrap_or_default();
            if !current_layout {
                seq = values.get(index + 2).cloned().unwrap_or_default();
            }
        }
        if id.is_empty() {
            continue;
        }
        let normalized_seq = seq.trim_start_matches('0');
        let allocation = allocations.get(&format!("{id}_{seq}")).or_else(|| {
            allocations
                .iter()
                .find(|(key, _)| {
                    key.split_once('_').is_some_and(|(code, candidate_seq)| {
                        code == id && candidate_seq.trim_start_matches('0') == normalized_seq
                    })
                })
                .map(|(_, value)| value)
        });
        let will_label = allocation
            .map(|value| match value.0.as_str() {
                "1" => "第一志愿",
                "2" => "第二志愿",
                "3" => "第三志愿",
                _ => "",
            })
            .filter(|value| !value.is_empty())
            .or_else(|| {
                values
                    .iter()
                    .find(|value| value.contains("志愿"))
                    .map(String::as_str)
            })
            .unwrap_or("");
        let will = allocation
            .and_then(|value| value.0.parse::<u64>().ok())
            .or(match will_label {
                "第一志愿" => Some(1),
                "第二志愿" => Some(2),
                "第三志愿" => Some(3),
                _ => None,
            });
        let type_code = allocation
            .map(|value| value.1.as_str())
            .or_else(|| {
                code_index
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| values.get(index))
                    .map(String::as_str)
            })
            .unwrap_or("");
        let is_sports =
            allocation.is_some_and(|value| value.2 == "是") || type_code.contains("体育");
        let type_label = if is_sports {
            "体育"
        } else {
            match type_code {
                "006" => "必修",
                "008" => "限选",
                "007" => "任选",
                "必修" => "必修",
                "限选" => "限选",
                "任选" => "任选",
                _ => "",
            }
        };
        let field = |offset: usize| {
            code_index
                .and_then(|index| index.checked_add(offset))
                .and_then(|index| values.get(index))
                .cloned()
                .unwrap_or_default()
        };
        let name = field(if current_layout { 2 } else { 1 });
        let time = field(3);
        let teacher = field(4);
        let credit = field(5);
        let secondary = field(6) == "是";
        rows.push(json!({
            "type": type_label,
            "will": will,
            "willLabel": will_label,
            "id": id,
            "name": name,
            "seq": seq,
            "time": time,
            "teacher": teacher,
            "credit": credit,
            "secondary": secondary,
            "cells": values
        }));
    }
    Ok(json!(rows))
}

fn whole_timetable_rows(html: &str) -> Result<Value> {
    use std::collections::BTreeMap;

    let block_re =
        Regex::new(r#"(?s)p_id=\d+;([A-Za-z0-9]+).{0,900}?getElementById\('a([1-6])_([1-7])'\)"#)?;
    let name_re = Regex::new(r#"<b>([^<&"'\n]{1,60})</b>"#)?;
    let part_re = Regex::new(r#"strHTML1\s*\+=\s*"；([^"]*)""#)?;
    let mut by_code = BTreeMap::<String, Value>::new();
    for capture in block_re.captures_iter(html) {
        let Some(whole_match) = capture.get(0) else {
            continue;
        };
        let code = capture.get(1).map(|value| value.as_str()).unwrap_or("");
        let section = capture.get(2).map(|value| value.as_str()).unwrap_or("");
        let day = capture.get(3).map(|value| value.as_str()).unwrap_or("");
        let mut block_end = (whole_match.end() + 600).min(html.len());
        while !html.is_char_boundary(block_end) {
            block_end -= 1;
        }
        let block = &html[whole_match.start()..block_end];
        let name = name_re
            .captures(block)
            .and_then(|capture| capture.get(1))
            .map(|capture| capture.as_str().trim().to_owned())
            .unwrap_or_default();
        if code.is_empty() || name.is_empty() {
            continue;
        }
        let parts = part_re
            .captures_iter(block)
            .filter_map(|capture| capture.get(1))
            .map(|capture| capture.as_str().trim().to_owned())
            .collect::<Vec<_>>();
        let teacher = parts.first().cloned().unwrap_or_default();
        let type_label = parts
            .iter()
            .skip(1)
            .find(|value| matches!(value.as_str(), "必修" | "限选" | "任选" | "体育"))
            .cloned()
            .unwrap_or_default();
        let weeks = parts
            .iter()
            .skip(1)
            .find(|value| {
                value.as_str() == "全周"
                    || (value.contains('周') && value.contains(|c: char| c.is_ascii_digit()))
            })
            .cloned()
            .unwrap_or_else(|| "全周".to_owned());
        let slot = format!("{day}-{section}({weeks})");
        by_code
            .entry(code.to_owned())
            .and_modify(|row| {
                if let Some(slots) = row["slots"].as_array_mut() {
                    slots.push(json!(slot));
                }
                if row["teacher"].as_str().unwrap_or("").is_empty() && !teacher.is_empty() {
                    row["teacher"] = json!(teacher);
                }
                if row["type"].as_str().unwrap_or("").is_empty() && !type_label.is_empty() {
                    row["type"] = json!(type_label);
                }
            })
            .or_insert_with(|| {
                json!({
                    "id": code,
                    "name": name,
                    "teacher": teacher,
                    "type": type_label,
                    "slots": [slot]
                })
            });
    }
    let rows = by_code
        .into_values()
        .map(|mut row| {
            let time = row["slots"]
                .as_array()
                .map(|slots| {
                    slots
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            row["time"] = json!(time);
            row
        })
        .collect::<Vec<_>>();
    Ok(json!(rows))
}

fn course_detail_fields(html: &str) -> Value {
    let mut fields = serde_json::Map::new();
    for row in Html::parse_document(html).select(&sel("table.table-striped tr")) {
        let values = cells(row);
        for pair in values.chunks(2) {
            let Some(label) = pair.first().filter(|value| !value.is_empty()) else {
                continue;
            };
            if label == "课程名"
                || label == "课程号"
                || label.chars().all(|character| character.is_ascii_digit())
            {
                continue;
            }
            let Some(value) = pair.get(1).filter(|value| !value.is_empty()) else {
                continue;
            };
            fields.insert(label.trim_end_matches('：').to_owned(), json!(value));
        }
    }
    Value::Object(fields)
}

fn footer(html: &str) -> Value {
    let txt = Html::parse_document(html)
        .select(&sel("p.yeM"))
        .map(text)
        .collect::<String>()
        .replace(',', "");
    if let Ok(re) = Regex::new(r"第\s*(\d+)\s*页\s*/\s*共\s*(\d+)\s*页（共\s*(\d+)\s*条记录）")
    {
        if let Some(c) = re.captures(&txt) {
            return json!({"page":c[1].parse::<u64>().ok(),"total_pages":c[2].parse::<u64>().ok(),"total_count":c[3].parse::<u64>().ok()});
        }
    }
    json!({"page":null,"total_pages":null,"total_count":null})
}
fn op(
    name: &str,
    mutating: bool,
    description: &str,
    required: &[&str],
    optional: &[&str],
    upstream: &str,
) -> Value {
    json!({"name":name,"mutating":mutating,"description":description,"required":required,"optional":optional,"upstream":upstream})
}
pub fn operations() -> Value {
    let mut a=vec![
op("passkey.list",false,"List server-side passkey registrations",&[],&[],"listPasskeys"),
op("passkey.remove",true,"Delete a server-side passkey registration",&["credential_id"],&[],"removePasskey"),
op("passkey.rename",true,"Rename a server-side passkey registration",&["credential_id","name"],&[],"renamePasskey"),
op("device.list",false,"List trusted ID devices",&[],&[],"getIdAuthDevices"),
op("device.remove",true,"Remove a specific trusted device",&["uuid"],&[],"forgetDevice"),
op("device.forget-thu-info",true,"Remove up to10 THU Info APP trusted devices until current fingerprint is no longer trusted",&["fingerprint"],&[],"forgetDevice"),
op("model.chat",true,"Send chat messages to campus MadModel and return response without executing tool calls",&["messages"],&["model","temperature","tools","max_tokens"],"apps/thu-info-app/src/ui/home/deepseek.tsx"),
op("info.model-token",false,"MadModel service access token (sensitive output)",&[],&[],"getMadModelToken"),
op("mail.send",true,"Send school mail through Coremail",&["subject","content","recipient"],&[],"naiveSendMail"),
op("dorm.recharge",true,"Create electricity recharge payment QR (does not complete payment)",&["amount"],&[],"getEleRechargePayCode"),
op("course.captcha",false,"Legacy course-registration captcha fallback as base64; modern SSO does not request it",&[],&[],"getCrCaptchaUrl"),
op("course.semesters",false,"Available registration semesters",&[],&["graduate"],"getCrAvailableSemesters"),
op("course.plan",false,"Personal course plan",&["semester"],&["graduate"],"getCoursePlan"),
op("course.search",false,"Course opening information and remaining capacity",&["semester"],&["graduate","page","id","name","teacher","day","period","department","grade","category","note","undergrad_available","graduate_available"],"searchCrCourses + NextTHUxk serverSearch"),
op("course.remaining",false,"Remaining course capacity",&["semester"],&["graduate","page","id","name","day","period"],"searchCrRemaining"),
op("course.open",false,"Course opening information",&["semester"],&["graduate","page","id","name","teacher","day","period","department","grade","category","note","undergrad_available","graduate_available"],"searchCrPrimaryOpen + NextTHUxk serverSearch"),
op("course.selected",false,"Selected courses",&["semester"],&["graduate"],"getSelectedCourses"),
op("course.select",true,"Submit a course selection with fresh CSRF token",&["semester","id","seq","priority","will"],&["graduate"],"selectCourse"),
op("course.delete",true,"Withdraw a selected course or leave the waiting queue",&["semester","id","seq"],&["graduate","candidate"],"deleteCourse + NextTHUxk dropCourse"),
op("course.will",true,"Change course selection preference",&["semester","id","seq","will"],&["graduate"],"changeCourseWill"),
op("course.stage",false,"Current registration stage",&["semester"],&["graduate"],"getCrCurrentStage"),
op("course.queue",false,"Courses currently in the waiting queue",&["semester"],&["graduate"],"getQueueInfo + NextTHUxk fetchCandidateCourses"),
op("course.priority-meta",false,"Course preference allocation stage",&["semester"],&["graduate"],"searchCoursePriorityMeta"),
op("course.priority",false,"Course preference statistics",&["semester"],&["graduate","sports","selected","page","id","seq","name","department"],"searchCoursePriorityInformation"),
op("course.pf-set",true,"Set elective course P/F",&["semester","id"],&["graduate"],"setCoursePF"),
op("course.pf-cancel",true,"Cancel elective course P/F",&["semester","id"],&["graduate"],"cancelCoursePF"),
op("course.timetable",false,"Registration deadlines",&[],&[],"getCrTimetable"),
op("course.schedule",false,"CR timetable as structured table rows",&["semester"],&["graduate"],"getCRSchedule"),
op("course.levels",false,"Course type map from the live primary timetable page",&["semester"],&["graduate"],"NextTHUxk fetchLevelTable"),
op("course.detail",false,"Read a course description from its detail URL identity",&["id","teacher_id"],&["graduate"],"NextTHUxk fetchCourseDetail"),
op("course.whole-schedule",false,"Whole registered/candidate timetable with normalized meeting slots",&["semester"],&["graduate"],"NextTHUxk fetchWholeTimetable"),
op("calendar.semesters",false,"Academic calendar current and next semesters",&[],&[],"getCalendar"),
op("calendar.year",false,"Academic calendar year",&[],&[],"getSchoolCalendarYear"),
op("calendar.image",false,"Academic calendar image URL",&["year","semester"],&["lang"],"getCalendarImageUrl"),
op("schedule.list",false,"Primary and secondary events for a date range",&["start","end"],&["graduate","first_day","semester","weeks"],"getSchedule"),
op("schedule.create",true,"Create a personal calendar entry",&["name","date","start","end"],&["location","graduate"],"saveCustomSchedule"),
op("schedule.delete",true,"Delete personal calendar entry by ID",&["id"],&["name","location","graduate"],"deleteCustomSchedule"),
op("info.user",false,"Unified ID account information",&[],&[],"getUserInfo/getIdAccountInfo"),
op("info.devices",false,"Unified ID trusted devices",&[],&[],"getIdAuthDevices"),
op("info.login-logs",false,"Unified ID login history",&[],&["page"],"getIdLoginLogs"),
op("info.report",false,"Academic grade report",&[],&["graduate","flag"],"getReport"),
op("info.physical",false,"Physical exam results",&[],&[],"getPhysicalExamResult"),
op("info.countdown",false,"Academic countdowns",&[],&[],"countdown"),
op("info.language",true,"Switch info portal language",&["lang"],&[],"switchLang"),
op("classroom.list",false,"Classroom building list",&[],&[],"getClassroomList"),
op("classroom.state",false,"Classroom availability",&["building","week"],&[],"getClassroomState"),
op("invoice.list",false,"Electronic invoice list",&[],&["page"],"getInvoiceList"),
op("invoice.pdf",false,"Electronic invoice PDF response",&["uuid"],&[],"getInvoicePDF"),
op("income.graduate",false,"Graduate income records",&["start","end"],&["page","rows"],"getGraduateIncome"),
op("income.bank",false,"Bank payment records by year",&["years"],&["foundation"],"getBankPayment/getBankPaymentParellize"),
op("program.completion",false,"Degree program completion tables",&[],&[],"getDegreeProgramCompletion"),
op("program.list",false,"Degree program list",&[],&[],"getFullDegreeProgram"),
op("program.detail",false,"Full degree program tables",&["id"],&[],"getFullDegreeProgram"),
op("assessment.list",false,"Course assessment forms",&[],&[],"getAssessmentList"),
op("assessment.form",false,"Read assessment form controls",&["path"],&[],"getAssessmentForm"),
op("assessment.submit",true,"Submit explicit assessment form fields",&["fields"],&[],"postAssessmentForm"),
op("dorm.electricity",false,"Dorm electricity balance",&[],&[],"getEleRemainder"),
op("dorm.payments",false,"Dorm electricity payment history",&[],&[],"getElePayRecord"),
op("dorm.score",false,"Dorm score chart URL",&[],&[],"getDormScore"),
op("dorm.password",true,"Reset dorm account password",&["new_password"],&[],"resetDormPassword"),
op("network.devices",false,"Online network devices",&[],&[],"getOnlineDevices"),
op("network.balance",false,"Network account balance",&[],&[],"getNetworkBalance"),
op("network.account",false,"Network account information",&[],&[],"getNetworkAccountInfo"),
op("network.login",true,"Authorize a network IP",&["ip","password"],&["internet"],"loginNetwork"),
op("network.logout",true,"Disconnect a network device",&["id","mac"],&[],"logoutNetwork"),
op("thos.score",false,"Course score from THOS",&["user_id","course_id"],&[],"getScoreByCourseId"),
op("thos.counts",false,"THOS service counters",&[],&[],"prepareThosSession"),
op("thos.tasks",false,"THOS work items",&["kind"],&["page","size","keyword"],"getThosTasks"),
op("thos.services",false,"THOS service catalogue",&[],&["page","size","keyword"],"getThosServices"),
];
    for (n, m, r, o, u) in [
        (
            "news.list",
            false,
            vec![],
            vec!["page", "length", "channel"],
            "getNewsList",
        ),
        (
            "news.search",
            false,
            vec!["keyword"],
            vec!["page", "channel_name", "exact"],
            "searchNewsList",
        ),
        (
            "news.detail",
            false,
            vec![],
            vec!["id", "url"],
            "getNewsDetail",
        ),
        (
            "news.favorites",
            false,
            vec![],
            vec!["page"],
            "getFavorNewsList",
        ),
        (
            "news.favorite-add",
            true,
            vec!["id"],
            vec![],
            "addNewsToFavor",
        ),
        (
            "news.favorite-remove",
            true,
            vec!["id"],
            vec![],
            "removeNewsFromFavor",
        ),
        (
            "news.subscriptions",
            false,
            vec![],
            vec![],
            "getNewsSubscriptionList",
        ),
        ("news.sources", false, vec![], vec![], "getNewsSourceList"),
        ("news.channels", false, vec![], vec![], "getNewsChannelList"),
        (
            "news.subscription-add",
            true,
            vec![],
            vec!["channel", "source", "keyword"],
            "addNewsSubscription",
        ),
        (
            "news.subscription-remove",
            true,
            vec!["id"],
            vec![],
            "removeNewsSubscription",
        ),
        (
            "news.subscribed",
            false,
            vec![],
            vec!["id", "page"],
            "getNewsListBySubscription",
        ),
        (
            "gitlab.namespaces",
            false,
            vec![],
            vec!["page"],
            "getNamespaces",
        ),
        (
            "gitlab.recent",
            false,
            vec![],
            vec!["page"],
            "getRecentProjects",
        ),
        (
            "gitlab.personal",
            false,
            vec!["name"],
            vec!["page"],
            "getPersonalProjects",
        ),
        (
            "gitlab.starred",
            false,
            vec![],
            vec!["page"],
            "getStarredProjects",
        ),
        (
            "gitlab.search",
            false,
            vec!["query"],
            vec!["page"],
            "searchProjects",
        ),
        (
            "gitlab.project",
            false,
            vec!["id"],
            vec![],
            "getProjectDetail",
        ),
        (
            "gitlab.tree",
            false,
            vec!["id", "ref"],
            vec!["path", "page"],
            "getProjectTree",
        ),
        (
            "gitlab.branches",
            false,
            vec!["id"],
            vec![],
            "getProjectBranches",
        ),
        (
            "gitlab.blob",
            false,
            vec!["id", "sha"],
            vec![],
            "getProjectFileBlob",
        ),
        (
            "gitlab.markdown",
            false,
            vec!["text"],
            vec![],
            "renderMarkdown",
        ),
        (
            "app.announcements",
            false,
            vec![],
            vec!["version"],
            "getLatestAnnounces",
        ),
        (
            "app.version",
            false,
            vec!["platform"],
            vec![],
            "getLatestVersion",
        ),
        ("app.feedbacks", false, vec![], vec![], "getFeedbackReplies"),
        (
            "app.feedback",
            true,
            vec![
                "content",
                "appversion",
                "os",
                "nickname",
                "contact",
                "phonemodel",
            ],
            vec![],
            "submitFeedback",
        ),
        (
            "app.qrcode",
            false,
            vec![],
            vec![],
            "getWeChatGroupQRCodeContent",
        ),
        ("app.privacy", false, vec![], vec![], "getPrivacyUrl"),
        (
            "app.startup-stat",
            true,
            vec!["uuid"],
            vec![],
            "appStartupStat",
        ),
        (
            "app.usage-stat",
            true,
            vec!["uuid", "usage"],
            vec![],
            "appUsageStat",
        ),
    ] {
        a.push(op(n, m, u, &r, &o, u));
    }
    a.extend(
        crate::thu_reservations::operations()
            .as_array()
            .into_iter()
            .flatten()
            .filter(|operation| {
                operation["name"]
                    .as_str()
                    .is_none_or(|name| !name.starts_with("sports."))
            })
            .cloned(),
    );
    a.extend(
        crate::thu_sports::operations()
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );
    a.extend(
        crate::thu_card::operations()
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );
    a.extend(crate::wsbx::operations());
    Value::Array(a)
}
pub fn roaming(operation: &str, a: &Value) -> Option<(&'static str, String)> {
    let (p, id) = if operation.starts_with("passkey.") || operation.starts_with("device.") {
        ("id_website", "")
    } else if operation.starts_with("course.")
        && operation != "course.timetable"
        && operation != "course.captcha"
    {
        ("cr", "")
    } else if operation == "calendar.semesters" {
        ("default", "3E401364BDD7AEA7EBF1EDE3F15ED4B7")
    } else if operation.starts_with("schedule.") {
        (
            "default",
            if flag(a, "graduate") {
                "BEABB32641DC4EC3510B048BAF42471A"
            } else {
                "287C0C6D90ABB364CD5FDF1495199962"
            },
        )
    } else if operation.starts_with("library.room-") {
        ("cab", "")
    } else if operation.starts_with("library.") {
        (
            "id",
            "ef84f6d6784f6b834e5214f432d6173f/0?/api/id_tsinghua_callback",
        )
    } else if operation.starts_with("card.") {
        ("card", "eea30cbedcaf97c69d28b2d92f22a259/0?/userindex")
    } else if operation.starts_with("gitlab.") {
        ("gitlab", "")
    } else if operation.starts_with("thos.") {
        ("default", "56B13DDF68BB3DEA13D98E1E3E776D3E")
    } else if operation.starts_with("network.") {
        ("default", "66D157166A3E5EEB3C558B66803B2929")
    } else if operation.starts_with("invoice.") {
        ("default", "625B81A7A9D148B01DA59185CC4074E1")
    } else if operation.starts_with("classroom.") {
        ("default", "40470BB47E0849E9EF717983490BC964")
    } else if operation.starts_with("assessment.") {
        ("default", "0D8B99BA23FD2BA22428D9C8AA0AB508")
    } else if operation.starts_with("program.") {
        ("default", "287C0C6D90ABB364CD5FDF1495199962")
    } else {
        match operation {
            "info.user" | "info.devices" | "info.login-logs" => ("id_website", ""),
            "info.report" => (
                "default",
                if flag(a, "graduate") {
                    "E35232808C08C8C5F199F13BF6B7F5D0"
                } else {
                    "B7EF0ADF9406335AD7905B30CD7B49B1"
                },
            ),
            "model.chat" | "info.model-token" => ("default", "19D04E39D96B36C494F2E48A1A4741FD"),
            "mail.send" => ("default", "F315577F5BF20E1B1668EDD594B2C04F"),
            "info.physical" => ("default", "8BF4F9A706589060488B6B6179E462E5"),
            "income.graduate" => ("default", "C0AE458CEACD0912982A09DDF0C136DA"),
            "income.bank" => (
                "default",
                if flag(a, "foundation") {
                    "C1ADD6B60D050B64E0C7B8F195CE89EC"
                } else {
                    "2A5182CB3F36E80395FC2091001BDEA6"
                },
            ),
            "dorm.score" => ("id", "0a993de7e533cd43a594459abdcab27d/0"),
            "dorm.electricity" | "dorm.payments" | "dorm.recharge" => {
                ("id", "0a993de7e533cd43a594459abdcab27d/1")
            }
            "dorm.password" => ("id", "051bb58cba58a1c5f67857606497387f"),
            _ => return None,
        }
    };
    Some((p, id.to_owned()))
}
pub async fn execute(session: &Session, operation: &str, a: &Value) -> Result<Value> {
    let c = &session.client;
    if operation.starts_with("passkey.") || operation.starts_with("device.") {
        return crate::auth::manage(c, operation, a).await;
    }
    if operation.starts_with("library.") {
        return crate::thu_reservations::execute(c, operation, a).await;
    }
    if operation.starts_with("sports.") {
        return crate::thu_sports::execute(session, operation, a).await;
    }
    if operation.starts_with("card.") {
        return crate::thu_card::execute(c, operation, a).await;
    }
    if operation.starts_with("wsbx.") {
        return crate::wsbx::execute(session, operation, a).await;
    }
    if operation.starts_with("course.") {
        return course(session, operation, a).await;
    }
    if operation.starts_with("schedule.") {
        return schedule(c, operation, a).await;
    }
    if operation.starts_with("news.") {
        return news(session, operation, a).await;
    }
    if operation.starts_with("gitlab.") {
        return gitlab(c, operation, a).await;
    }
    if operation.starts_with("network.") {
        return network(c, operation, a).await;
    }
    if operation.starts_with("thos.") {
        return thos(c, operation, a).await;
    }
    match operation {
        "session.logout-remote" => {
            c.get(LOGOUT_URL).send().await?.error_for_status()?;
            Ok(
                json!({"webvpn_logout_requested":true,"note":"Other target sessions may remain; clear local cookies separately"}),
            )
        }
        "model.chat" => model_chat(c, a).await,
        "info.model-token" => {
            let h = crate::auth::roam(
                c,
                &crate::auth::RoamOptions {
                    policy: "default".to_owned(),
                    payload: "19D04E39D96B36C494F2E48A1A4741FD".to_owned(),
                    credentials: None,
                },
            )
            .await?;
            let v: Value = serde_json::from_str(&h)?;
            Ok(json!({"token":v["data"],"sensitive":true}))
        }
        "mail.send" => send_mail(c, a).await,
        "dorm.recharge" => recharge_dorm(c, a).await,
        "calendar.year" => jget(c, CALENDAR_YEAR_URL).await,
        "calendar.image" => {
            let term = s(a, "semester")?;
            if term != "spring" && term != "autumn" {
                bail!("semester must be spring or autumn");
            }
            let lang = d(a, "lang", "zh");
            if lang != "zh" && lang != "en" {
                bail!("lang must be zh or en");
            }
            Ok(
                json!({"url":format!("{}/{}/{}-{}.jpg",CALENDAR_IMAGE_URL,lang,s(a,"year")?,if term=="spring"{2}else{1})}),
            )
        }
        "calendar.semesters" => {
            let home = get(c, LEARN_HOME_URL).await?;
            let token = capture(&home, r"_csrf=([\w-]+)")?;
            let data = jget(c, &format!("{SEMESTER_LIST_URL}{token}")).await?;
            if data["message"] != "success" {
                bail!("Calendar API rejected request");
            }
            Ok(
                json!({"current":parse_semester(&data["result"])?,"next":data["resultList"].as_array().context("Missing resultList")?.iter().map(parse_semester).collect::<Result<Vec<_>>>()?}),
            )
        }
        "info.user" => {
            let h = get(c, ID_ACCOUNT_SETTINGS_URL).await?;
            let v: Value = serde_json::from_str(&capture(&h, r#""account"\s*:\s*(\{[^}]*\})"#)?)?;
            if v["username"].as_str().unwrap_or("").is_empty() {
                bail!("Not signed in to unified ID");
            }
            Ok(v)
        }
        "info.devices" => {
            let v: Value =
                serde_json::from_str(&post(c, GET_DEVICE_LIST_URL, &json!({}), false).await?)?;
            if v["result"] != "success" {
                bail!("Unable to read trusted devices");
            }
            Ok(v["object"].clone())
        }
        "info.login-logs" => {
            jget(
                c,
                &query(ID_LOGIN_LOG_DATA_URL, &json!({"page":d(a,"page","1")}))?,
            )
            .await
        }
        "info.physical" => jget(c, PHYSICAL_EXAM_URL).await,
        "info.report" => {
            let u = crurl(a, GET_BKS_REPORT_URL, GET_YJS_REPORT_URL);
            let h = get(
                c,
                &query(u, &json!({"flag":format!("di{}",d(a,"flag","1"))}))?,
            )
            .await?;
            let rows=Html::parse_document(&h).select(&sel("table[cellspacing='1'] tr")).skip(1).map(|e|{let t=cells(e);json!({"id":field_value(&t,0),"name":field_value(&t,1),"credit":field_value(&t,2),"grade":field_value(&t,if flag(a,"graduate"){4}else{3}),"point":field_value(&t,if flag(a,"graduate"){5}else{4}),"semester":field_value(&t,if flag(a,"graduate"){6}else{5})})}).collect::<Vec<_>>();
            if rows.is_empty() && !h.contains("table1") {
                bail!("Grade report structure not found");
            }
            Ok(json!(rows))
        }
        "info.countdown" => Ok(json!(Html::parse_document(&get(c, COUNT_DOWN_URL).await?)
            .select(&sel(".countdown li"))
            .map(text)
            .collect::<Vec<_>>())),
        "info.language" => {
            let lang = s(a, "lang")?;
            if lang != "zh" && lang != "en" {
                bail!("lang must be zh or en");
            }
            get(
                c,
                &format!(
                    "{SWITCH_LANG_URL}{}&_csrf={}",
                    if lang == "zh" { "awefawef" } else { "en_US" },
                    csrf(session).await?
                ),
            )
            .await?;
            Ok(json!({"updated":true}))
        }
        "classroom.list" => {
            let h = get(c, CLASSROOM_LIST_URL).await?;
            let re = Regex::new(r"classroom=(.+?)&weeknumber=(\d+)")?;
            Ok(json!(Html::parse_document(&h).select(&sel(".w30 a")).filter_map(|e|{let caps=re.captures(e.value().attr("href")?)?;Some(json!({"name":text(e),"searchName":&caps[1],"weekNumber":caps[2].parse::<u32>().ok()}))}).collect::<Vec<_>>()))
        }
        "classroom.state" => {
            let h = get(
                c,
                &format!(
                    "{CLASSROOM_STATE_PREFIX}{}{CLASSROOM_STATE_MIDDLE}{}",
                    encoded(&s(a, "building")?, true),
                    s(a, "week")?
                ),
            )
            .await?;
            let doc = Html::parse_document(&h);
            let states=doc.select(&sel("#scrollContent table tbody tr")).map(|e|{let cs=e.select(&sel("td")).collect::<Vec<_>>();json!({"name":cs.first().map(|e|text(*e)),"status":cs.iter().skip(1).map(|e|{let class=e.value().attr("class").unwrap_or("");if class.contains("onteaching"){"teaching"}else if class.contains("onexam"){"exam"}else if class.contains("onborrowed"){"borrowed"}else if class.contains("ondisabled"){"disabled"}else{"available"}}).collect::<Vec<_>>()})}).collect::<Vec<_>>();
            if !h.contains("scrollContent") {
                bail!("Classroom state table absent");
            }
            Ok(
                json!({"week":a["week"],"validWeeks":doc.select(&sel("#weeknumber option")).filter_map(|e|e.value().attr("value")).collect::<Vec<_>>(),"dates":doc.select(&sel("[colspan='6']")).take(7).map(text).collect::<Vec<_>>(),"classrooms":states}),
            )
        }
        "invoice.list" => Ok(serde_json::from_str(
            &post(
                c,
                INVOICE_LIST_URL,
                &json!({"page":d(a,"page","1"),"limit":20,"columnName":"inv_date","sort":"desc"}),
                false,
            )
            .await?,
        )?),
        "invoice.pdf" => {
            let u = format!("{INVOICE_CONTENT_URL}{}", encoded(&s(a, "uuid")?, false));
            let b = c.get(u).send().await?.error_for_status()?.bytes().await?;
            use base64::Engine;
            Ok(
                json!({"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(b)}),
            )
        }
        "income.graduate" => {
            let v:Value=serde_json::from_str(&post(c,GRADUATE_INCOME_URL,&json!({"ffkssj":s(a,"start")?,"ffjssj":s(a,"end")?,"_search":false,"rows":d(a,"rows","1000"),"page":d(a,"page","1"),"sidx":"id","sord":"asc"}),false).await?)?;
            Ok(v)
        }
        "income.bank" => {
            let years = a["years"].as_array().context("years must be an array")?;
            let b = years
                .iter()
                .map(|x| {
                    let y = match x {
                        Value::String(v) => v.clone(),
                        Value::Number(v) => v.to_string(),
                        _ => bail!("years entries must be strings or integers"),
                    };
                    if y.is_empty() {
                        bail!("years cannot contain empty entries");
                    }
                    Ok(format!("year={}", encoded(&y, false)))
                })
                .collect::<Result<Vec<_>>>()?
                .join("&");
            let h = body(
                c.post(if flag(a, "foundation") {
                    FOUNDATION_BANK_PAYMENT_SEARCH_URL
                } else {
                    BANK_PAYMENT_SEARCH_URL
                })
                .header("content-type", "application/x-www-form-urlencoded")
                .body(b)
                .send()
                .await?,
            )
            .await?;
            Ok(
                json!({"headings":Html::parse_document(&h).select(&sel("div strong")).map(text).collect::<Vec<_>>(),"rows":tables(&h,"table tr")}),
            )
        }
        "program.completion" | "program.list" | "program.detail" => {
            let u = match operation {
                "program.completion" => PROGRAM_URL.to_owned(),
                "program.list" => PROGRAM_LIST_URL.to_owned(),
                _ => format!("{FULL_PROGRAM_URL_PREFIX}{}", encoded(&s(a, "id")?, false)),
            };
            let h = get(c, &u).await?;
            Ok(
                json!({"tables":tables(&h,"table tr"),"links":Html::parse_document(&h).select(&sel("a[href]")).map(|e|json!({"label":text(e),"href":e.value().attr("href")})).collect::<Vec<_>>()}),
            )
        }
        "assessment.list" => {
            let h = get(c, ASSESSMENT_LIST_URL).await?;
            if h.contains("对不起，现在不是填写问卷时间") {
                bail!("Not an assessment period");
            }
            let re = Regex::new(r"Body\('(.+?)'\)")?;
            Ok(json!(Html::parse_document(&h)
                .select(&sel("tbody tr"))
                .map(|e| {
                    let t = cells(e);
                    let links = e
                        .select(&sel("[onclick]"))
                        .filter_map(|n| {
                            re.captures(n.value().attr("onclick")?)
                                .map(|x| x[1].to_owned())
                        })
                        .collect::<Vec<_>>();
                    json!({"cells":t,"paths":links})
                })
                .collect::<Vec<_>>()))
        }
        "assessment.form" => {
            let path = s(a, "path")?;
            if !path.starts_with("/jxpg/") {
                bail!("path must begin /jxpg/");
            }
            let h = get(c, &format!("{ASSESSMENT_BASE_URL}{path}")).await?;
            Ok(parse_controls(&h))
        }
        "assessment.submit" => {
            let v: Value =
                serde_json::from_str(&post(c, ASSESSMENT_SUBMIT_URL, &a["fields"], false).await?)?;
            if v["result"] != "success" {
                bail!("Assessment submission rejected: {}", v["msg"]);
            }
            Ok(v)
        }
        "dorm.electricity" => {
            let h = get(c, ELE_REMAINDER_URL).await?;
            let doc = Html::parse_document(&h);
            let val = doc
                .select(&sel("#Netweb_Home_electricity_DetailCtrl1_lblele"))
                .next()
                .map(text)
                .context("Electricity balance absent")?;
            Ok(
                json!({"remainder":val.parse::<f64>()?,"updateTime":doc.select(&sel("#Netweb_Home_electricity_DetailCtrl1_lbltime")).next().map(text)}),
            )
        }
        "dorm.payments" => {
            let h = get(c, ELE_PAY_RECORD_URL).await?;
            Ok(tables(&h, ".myTable tr"))
        }
        "dorm.score" => {
            let h = get(c, DORM_SCORE_URL).await?;
            let doc = Html::parse_document(&h);
            let path = doc
                .select(&sel("#weixin_health_linechartCtrl1_Chart1"))
                .next()
                .and_then(|x| x.value().attr("src"))
                .context("Dorm score chart not found")?;
            Ok(json!({"url":reqwest::Url::parse(WEB_VPN_ROOT_URL)?.join(path)?.to_string()}))
        }
        "dorm.password" => {
            let h = get(c, CHANGE_HOME_PASSWORD_URL).await?;
            if !h.contains("ChangePasswordCtrl1_txtoldpassword") {
                bail!("Password form absent");
            }
            let mut f = hidden_all(&h);
            f["__EVENTTARGET"] = json!("ChangePasswordCtrl1:btnOK");
            f["ChangePasswordCtrl1$txtoldpassword"] = json!("");
            f["ChangePasswordCtrl1$txtnewpassword"] = json!(s(a, "new_password")?);
            f["ChangePasswordCtrl1$txtnewpassword1"] =
                f["ChangePasswordCtrl1$txtnewpassword"].clone();
            let r = post(c, CHANGE_HOME_PASSWORD_URL, &f, false).await?;
            Ok(
                json!({"response_text":Html::parse_document(&r).root_element().text().collect::<String>(),"verification":"server response returned; verify status before retry"}),
            )
        }
        "app.announcements" => {
            jget(
                c,
                &format!(
                    "{APP_ANNOUNCEMENT_URL}{}",
                    encoded(&d(a, "version", "0.0.0"), false)
                ),
            )
            .await
        }
        "app.version" => {
            let p = s(a, "platform")?;
            if p != "ios" && p != "android" {
                bail!("platform must be ios or android");
            }
            jget(c, &format!("{APP_LATEST_VERSION_URL}/{p}")).await
        }
        "app.feedbacks" => jget(c, APP_FEEDBACKS_URL).await,
        "app.feedback" => {
            let v = json!({"content":s(a,"content")?,"appversion":s(a,"appversion")?,"os":s(a,"os")?,"nickname":s(a,"nickname")?,"contact":s(a,"contact")?,"phonemodel":s(a,"phonemodel")?});
            let r = body(c.post(APP_SUBMIT_FEEDBACK_URL).json(&v).send().await?).await?;
            Ok(json!({"response":r}))
        }
        "app.qrcode" => Ok(json!({"content":get(c,APP_QRCODE_URL).await?})),
        "app.privacy" => Ok(json!({"url":APP_PRIVACY_URL})),
        "app.startup-stat" | "app.usage-stat" => {
            let v = if operation == "app.startup-stat" {
                json!({"Uuid":s(a,"uuid")?})
            } else {
                json!({"Uuid":s(a,"uuid")?,"Function":a["usage"]})
            };
            let r = body(
                c.post(if operation == "app.startup-stat" {
                    APP_STARTUP_STAT_URL
                } else {
                    APP_USAGE_STAT_URL
                })
                .json(&v)
                .send()
                .await?,
            )
            .await?;
            Ok(json!({"response":r}))
        }
        _ => bail!("Unknown THU operation {operation}"),
    }
}
fn parse_controls(html: &str) -> Value {
    json!({"hidden":hidden_all(html),"controls":Html::parse_document(html).select(&sel("input,textarea,select")).filter_map(|e|{let name=e.value().attr("name")?;Some(json!({"name":name,"type":e.value().attr("type"),"value":e.value().attr("value").unwrap_or(""),"text":text(e),"options":e.select(&sel("option")).map(|o|json!({"value":o.value().attr("value"),"text":text(o),"selected":o.value().attr("selected").is_some()})).collect::<Vec<_>>()}))}).collect::<Vec<_>>()})
}
fn parse_semester(v: &Value) -> Result<Value> {
    use chrono::{Datelike, Duration, NaiveDate};
    let start = s(v, "kssj")?;
    let end = s(v, "jssj")?;
    let dt = NaiveDate::parse_from_str(start.get(..10).context("Bad semester start")?, "%Y-%m-%d")?;
    let weekday = dt.weekday().num_days_from_monday();
    let delta = if weekday >= 5 {
        7 - weekday as i64
    } else {
        -(weekday as i64)
    };
    let first = dt + Duration::days(delta);
    let last = NaiveDate::parse_from_str(end.get(..10).context("Bad semester end")?, "%Y-%m-%d")?;
    Ok(
        json!({"first_day":first.to_string(),"semester":v["id"],"name":v["xnxqmc"],"weeks":(last-first).num_days()/7+1}),
    )
}
async fn course(session: &Session, op: &str, a: &Value) -> Result<Value> {
    let c = &session.client;
    let select = crurl(a, CR_SELECT_URL, CR_SELECT_YJS_URL);
    let search = crurl(a, CR_SEARCH_URL, CR_SEARCH_YJS_URL);
    let semester = d(a, "semester", "");
    if op != "course.semesters"
        && op != "course.timetable"
        && op != "course.captcha"
        && op != "course.detail"
        && semester.is_empty()
    {
        bail!("semester is required");
    }
    match op {
        "course.captcha" => {
            get(c, CR_LOGIN_HOME_URL).await?;
            use base64::Engine;
            let b = c
                .get(CR_CAPTCHA_URL)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            Ok(
                json!({"encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(b),"requires_manual_solution":true}),
            )
        }
        "course.semesters" => {
            let h = get(c, crurl(a, CR_MAIN_URL, CR_MAIN_YJS_URL)).await?;
            let sem = capture(&h, r"m=showTree&p_xnxq=(\d{4}-\d{4}-\d)")?;
            let h = get(
                c,
                &format!("{}{sem}", crurl(a, CR_TREE_URL, CR_TREE_YJS_URL)),
            )
            .await?;
            Ok(json!(Html::parse_document(&h)
                .select(&sel("option"))
                .map(|e| json!({"id":e.value().attr("value"),"name":text(e)}))
                .collect::<Vec<_>>()))
        }
        "course.plan" => {
            let u = if flag(a, "graduate") {
                COURSE_PLAN_YJS_URL.to_string()
            } else {
                format!("{COURSE_PLAN_URL_PREFIX}{}", encoded(&semester, false))
            };
            let h = get(c, &u).await?;
            let rows=Html::parse_document(&h).select(&sel(".trr2")).map(|e|{let mut t=cells(e);if t.len()==7&&!flag(a,"graduate"){t.drain(..2);}
if flag(a,"graduate"){json!({"id":field_value(&t,1),"name":field_value(&t,2),"credit":field_value(&t,3),"property":field_value(&t,5),"group":field_value(&t,6)})}else{json!({"id":field_value(&t,0),"name":field_value(&t,1),"property":field_value(&t,2),"credit":field_value(&t,3),"group":field_value(&t,4)})}}).collect::<Vec<_>>();
            Ok(json!(rows))
        }
        "course.remaining" | "course.open" | "course.search" => {
            let mut f = json!({"m":"kylSearch","page":d(a,"page","-1"),"p_sort.p1":"","p_sort.p2":"","p_sort.asc1":"true","p_sort.asc2":"true","p_xnxq":semester,"pathContent":"课余量查询","p_kch":d(a,"id",""),"p_kxh":"","p_kcm":d(a,"name",""),"p_skxq":d(a,"day",""),"p_skjc":d(a,"period",""),"goPageNumber":d(a,"page","1")});
            let mut open_result = Value::Null;
            if op != "course.remaining" {
                f["m"] = json!("kkxxSearch");
                f["pathContent"] = json!("一级课开课信息");
                for key in ["showtitle", "p_kcflm", "p_xkwzsm"] {
                    f[key] = json!("");
                }
                f["p_zjjsxm"] = json!(d(a, "teacher", ""));
                f["p_rxklxm"] = json!(d(a, "category", ""));
                f["p_kkdwnm"] = json!(d(a, "department", ""));
                f["p_kctsm"] = json!(d(a, "note", ""));
                f["p_ssnj"] = json!(d(a, "grade", ""));
                f["p_bkskyl_ig"] = json!(if flag(a, "undergrad_available") {
                    "0"
                } else {
                    ""
                });
                f["p_yjskyl_ig"] = json!(if flag(a, "graduate_available") {
                    "0"
                } else {
                    ""
                });
                let h = post(c, search, &f, true).await?;
                open_result = footer(&h);
                open_result["courses"] = course_catalog_rows(&h);
                if op == "course.open" {
                    return Ok(open_result);
                }
            }
            f["m"] = json!("kylSearch");
            f["pathContent"] = json!("课余量查询");
            let h = post(c, search, &f, true).await?;
            let mut r = footer(&h);
            let rows=Html::parse_document(&h).select(&sel(".trr2")).map(|e|{let t=cells(e);let q=t.len()==8;json!({"id":field_value(&t,0),"seq":field_value(&t,1),"name":field_value(&t,2),"capacity":field_value(&t,3),"remaining":field_value(&t,4),"queue":if q{field_value(&t,5)}else{Value::Null},"teacher":field_value(&t,if q{6}else{5}),"time":field_value(&t,if q{7}else{6})})}).collect::<Vec<_>>();
            if op == "course.remaining" {
                r["courses"] = json!(rows);
                return Ok(r);
            }
            if let Some(items) = open_result["courses"].as_array_mut() {
                for it in items {
                    if let Some(rem) = rows.iter().find(|v| {
                        v["id"] == it["id"]
                            && normalized_sequence(&v["seq"]) == normalized_sequence(&it["seq"])
                    }) {
                        for key in ["capacity", "remaining", "queue"] {
                            it[key] = rem[key].clone();
                        }
                    } else {
                        it["capacity"] = it[if flag(a, "graduate") {
                            "yjsCapacity"
                        } else {
                            "bksCapacity"
                        }]
                        .clone();
                        it["remaining"] = Value::Null;
                        it["queue"] = Value::Null;
                    }
                }
            }
            Ok(open_result)
        }
        "course.selected" | "course.delete" | "course.will" => {
            if op == "course.delete" && flag(a, "candidate") {
                let h = get(
                    c,
                    &query(select, &json!({"m":"dlSearchTab","p_xnxq":semester}))?,
                )
                .await?;
                let f = json!({
                    "m": "dlDelete",
                    "token": hidden(&h, "token")?,
                    "p_xnxq": semester,
                    "page": "",
                    "p_del_id": format!("{};{};{};", semester, s(a, "id")?, s(a, "seq")?)
                });
                return message(&post(c, select, &f, false).await?);
            }
            let h = get(
                c,
                &query(
                    select,
                    &json!({"m":"yxSearchTab","p_xnxq":semester,"tokenPriFlag":"yx"}),
                )?,
            )
            .await?;
            if op == "course.selected" {
                return selected_course_rows(&h);
            }
            let mut f = json!({"m":"deleteYxk","token":hidden(&h,"token")?,"p_xnxq":semester,"tokenPriFlag":"yx"});
            if op == "course.delete" {
                f["p_del_id"] = json!(format!("{};{};{};", semester, s(a, "id")?, s(a, "seq")?));
            } else {
                let will = s(a, "will")?;
                if !["1", "2", "3"].contains(&will.as_str()) {
                    bail!("will must be 1,2,3");
                }
                f["m"] = json!("changeZY");
                f["jhzy_kch"] = json!(s(a, "id")?);
                f["jhzy_kxh"] = json!(s(a, "seq")?);
                f["jhzy_zy"] = json!(will);
            }
            message(&post(c, select, &f, false).await?)
        }
        "course.select" => {
            let priority = s(a, "priority")?;
            if !["bx", "xx", "rx", "ty", "xwk", "fxwk", "tyk", "cx"].contains(&priority.as_str()) {
                bail!("Invalid course priority");
            }
            let will = s(a, "will")?;
            if !["1", "2", "3"].contains(&will.as_str()) {
                bail!("will must be 1,2,3");
            }
            let h=get(c,&query(select,&json!({"m":format!("{priority}Search"),"p_xnxq":semester,"tokenPriFlag":priority}))?).await?;
            let token = hidden(&h, "token")?;
            let field = match priority.as_str() {
                "rx" => "rx".to_owned(),
                "ty" => "rxTy".to_owned(),
                _ => format!("{priority}k"),
            };
            let mut f = json!({"m":format!("save{}{}Kc",priority[..1].to_uppercase(),&priority[1..2]),"token":token,"p_xnxq":semester,"tokenPriFlag":priority});
            f[format!("p_{field}_id")] =
                json!(format!("{};{};{};", semester, s(a, "id")?, s(a, "seq")?));
            f[format!("p_{field}_xkzy")] = json!(will);
            let response = post(c, select, &f, false).await?;
            if let Ok(result) = message(&response) {
                return Ok(json!({"message": result["message"], "queued": false}));
            }
            if response.contains("是否排队") && response.contains("saveBksKcDl") {
                f["m"] = json!("saveBksKcDl");
                f["token"] = json!(hidden(&response, "token")?);
                let queued_response = post(c, select, &f, false).await?;
                let result = message(&queued_response)?;
                return Ok(json!({"message": result["message"], "queued": true}));
            }
            bail!(
                "Selection response was neither a result message nor a queue confirmation; inspect the selected and waiting-queue lists before retrying"
            )
        }
        "course.stage" => {
            let h = get(
                c,
                &format!(
                    "{select}?m=selectKc&p_xnxq={}&pathContent={}",
                    encoded(&semester, false),
                    encoded("一级选课", true)
                ),
            )
            .await?;
            Ok(
                json!({"stage":capture(&h,r#""当前选课阶段：(.+?)&nbsp;&nbsp;""#)?,"begin":capture(&h,r#""(.+?)开始&nbsp;&nbsp;""#)?,"end":capture(&h,r#""(.+?)结束""#)?}),
            )
        }
        "course.queue" => {
            let h = get(
                c,
                &format!(
                    "{select}?m=dlSearch&p_xnxq={}&pathContent={}",
                    encoded(&semester, false),
                    encoded("队列信息查询", true)
                ),
            )
            .await?;
            Ok(named_rows(
                &h,
                ".trr2",
                &[
                    (0, "property"),
                    (1, "will"),
                    (2, "id"),
                    (3, "name"),
                    (4, "seq"),
                    (5, "inQueue"),
                    (6, "position"),
                    (7, "time"),
                    (8, "teacher"),
                ],
            ))
        }
        "course.priority-meta" => {
            let h = get(
                c,
                &query(select, &json!({"m":"xkqkSearch","p_xnxq":semester}))?,
            )
            .await?;
            let doc = Html::parse_document(&h);
            Ok(
                json!({"current":doc.select(&sel(".pad font")).map(text).collect::<String>(),"text":doc.select(&sel(".pad")).map(text).collect::<String>()}),
            )
        }
        "course.priority" => {
            let u = crurl(a, CR_ZYTJB_URL, CR_ZYTJB_YJS_URL);
            let mode = if flag(a, "sports") {
                "tbzySearchTy"
            } else {
                "tbzySearchBR"
            };
            let h = if flag(a, "selected") {
                get(
                    c,
                    &query(u, &json!({"m":mode,"p_xnxq":semester,"type":"GR"}))?,
                )
                .await?
            } else {
                let initial = get(c, &query(u, &json!({"m":mode,"p_xnxq":semester}))?).await?;
                let mut f = json!({"m":mode,"page":d(a,"page","-1"),"token":hidden(&initial,"token")?,"p_xnxq":semester,"p_kch":d(a,"id",""),"p_kxh":d(a,"seq",""),"p_kcm":d(a,"name",""),"p_lrdwnm":d(a,"department","")});
                if flag(a, "sports") {
                    f["p_tbzyType"] = json!(mode);
                } else {
                    f["tokenPriFlag"] = json!("yx");
                }
                post(c, u, &f, true).await?
            };
            Ok(json!({"pagination":footer(&h),"rows":tables(&h,"#content_1 tr")}))
        }
        "course.pf-set" | "course.pf-cancel" => {
            get(c, &query(select, &json!({"m":"pfkcxz","p_xnxq":semester}))?).await?;
            let h = get(
                c,
                &query(
                    select,
                    &json!({"m":"yxpfxz","p_xnxq":semester,"tokenPriFlag":"yx"}),
                )?,
            )
            .await?;
            let id = s(a, "id")?;
            let doc = Html::parse_document(&h);
            let set = op == "course.pf-set";
            let row = doc
                .select(&sel(if set {
                    ".tabdiv #content_1 .table1 tr"
                } else {
                    ".xinXi2 #content_1 .table1 tr"
                }))
                .find(|e| cells(*e).get(if set { 2 } else { 1 }) == Some(&id))
                .context("Course not offered for this P/F operation")?;
            let mut f = hidden_all(&h);
            f["token"] = json!(hidden(&h, "token")?);
            if set {
                f["p_pf_id"] = json!(row
                    .select(&sel("input[type=radio]"))
                    .next()
                    .and_then(|x| x.value().attr("value"))
                    .context("Course cannot be set P/F")?);
            }
            f["m"] = json!(if set { "editpfyes" } else { "editpfcancle" });
            let result = message(&post(c, select, &f, false).await?)?;
            let expected = if set {
                "任选课置为P/F成功"
            } else {
                "任选课取消置为P/F成功"
            };
            if result["message"] != expected {
                bail!("P/F change rejected: {}", result["message"]);
            }
            Ok(result)
        }
        "course.timetable" => {
            jget(
                c,
                &query(DEADLINE_URL, &json!({"_csrf":csrf(session).await?}))?,
            )
            .await
        }
        "course.schedule" => {
            let h = get(
                c,
                &format!(
                    "{select}?m=kbSearch&p_xnxq={}&pathContent={}",
                    encoded(&semester, false),
                    encoded("一级课表", true)
                ),
            )
            .await?;
            Ok(
                json!({"rows":tables(&h,"table tr"),"format":"source timetable table; not expanded events"}),
            )
        }
        "course.levels" => {
            let h = get(
                c,
                &format!(
                    "{select}?p_xnxq={}&pathContent={}",
                    encoded(&semester, false),
                    encoded("一级课表", true)
                ),
            )
            .await?;
            Ok(json!({
                "courses": course_level_rows(&h),
                "format": "course type map parsed from the live primary timetable"
            }))
        }
        "course.detail" => {
            let teacher_id = s(a, "teacher_id")?;
            let id = s(a, "id")?;
            let identity = format!("{teacher_id};{id}");
            let h = get(
                c,
                &format!(
                    "{CR_COURSE_DETAIL_URL}?m=showToXs&p_id={}",
                    encoded(&identity, false)
                ),
            )
            .await?;
            Ok(course_detail_fields(&h))
        }
        "course.whole-schedule" => {
            let h = get(
                c,
                &format!(
                    "{CR_WHOLE_TIMETABLE_URL}?m=ztkbSearch&p_xnxq={}&pathContent={}",
                    encoded(&semester, false),
                    encoded("整体课表", true)
                ),
            )
            .await?;
            Ok(json!({
                "courses": whole_timetable_rows(&h)?,
                "format": "normalized course meeting slots from ztkbSearch"
            }))
        }
        _ => bail!("Unknown course operation"),
    }
}
async fn schedule(c: &Client, op: &str, a: &Value) -> Result<Value> {
    use chrono::{Duration, NaiveDate, NaiveTime};
    if op == "schedule.list" {
        let start = NaiveDate::parse_from_str(&s(a, "start")?, "%Y-%m-%d")?;
        let end = NaiveDate::parse_from_str(&s(a, "end")?, "%Y-%m-%d")?;
        if end < start || (end - start).num_days() > 400 {
            bail!("Date range must be ordered and at most400 days");
        }
        let mut events = Vec::new();
        let mut day = start;
        while day <= end {
            let last = (day + Duration::days(20)).min(end);
            let u = format!(
                "{}{}{JXRL_MIDDLE}{}{JXRL_SUFFIX}",
                crurl(a, JXRL_BKS_PREFIX, JXRL_YJS_PREFIX),
                day.format("%Y%m%d"),
                last.format("%Y%m%d")
            );
            let h = get(c, &u).await?;
            let value = parse_jsonp(&h)?;
            for e in value
                .as_array()
                .context("Schedule JSONP must contain array")?
            {
                events.push(json!({"name":e["nr"],"location":e["dd"],"category":e["fl"],"id":e["grrlID"],"date":e["nq"],"start":e["kssj"].as_str().unwrap_or("").replace('：',":"),"end":e["jssj"].as_str().unwrap_or("").replace('：',":"),"source":"primary"}));
            }
            day = last + Duration::days(1);
        }
        let mut warnings = vec![];
        if !flag(a, "graduate") {
            if let Some(first) = a["first_day"].as_str() {
                let h = get(c, SECONDARY_URL).await?;
                let (mut sec, unparsed) = parse_secondary(&h, first)?;
                sec.retain(|e| {
                    let dt = e["date"].as_str().unwrap_or("");
                    dt >= start.to_string().as_str() && dt <= end.to_string().as_str()
                });
                events.extend(sec);
                if unparsed > 0 {
                    warnings.push(format!(
                        "{unparsed} secondary entries could not be parsed; output is incomplete"
                    ));
                }
            } else {
                warnings.push("Secondary timetable requires first_day (semester Monday); omitted because not supplied".to_owned());
            }
        }
        if let Some(sem) = a["semester"].as_str() {
            if sem.ends_with("-3") {
                let first = s(a, "first_day")?;
                let weeks = d(a, "weeks", "16").parse::<u32>()?;
                crate::auth::roam(
                    c,
                    &crate::auth::RoamOptions {
                        policy: "cr".to_owned(),
                        payload: String::new(),
                        credentials: None,
                    },
                )
                .await?;
                let h = get(
                    c,
                    &query(
                        crurl(a, CR_SELECT_URL, CR_SELECT_YJS_URL),
                        &json!({"m":"kbSearch","p_xnxq":sem,"pathContent":"一级课表"}),
                    )?,
                )
                .await?;
                let (mut extra, bad) = parse_cr_schedule(&h, &first, weeks)?;
                extra.retain(|e| {
                    let dt = e["date"].as_str().unwrap_or("");
                    dt >= start.to_string().as_str() && dt <= end.to_string().as_str()
                });
                events.extend(extra);
                if bad > 0 {
                    warnings.push(format!("{bad} CR timetable blocks unparsed"));
                }
            }
        } else {
            warnings.push(
                "semester not supplied: summer CR timetable fallback could not be determined"
                    .to_owned(),
            );
        }
        Ok(
            json!({"events":events,"warnings":warnings,"timezone":"Asia/Shanghai","complete":warnings.is_empty(),"note":"Primary daily items are not merged. Summer semester CR fallback is included when semester ends in -3."}),
        )
    } else {
        let h = get(c, crurl(a, JXRL_BKS_URL, JXRL_YJS_URL)).await?;
        if !h.contains("jxmh.do") {
            bail!("Personal calendar form not found");
        }
        let mut f = json!({"m":if op=="schedule.create"{"saveGrrl"}else{"deleteGrrl"},"role":hidden(&h,"role")?,"token":hidden(&h,"token")?,"grrlID":"","displayType":"","zt":d(a,"name",""),"dd":d(a,"location","")});
        if op == "schedule.create" {
            let date = NaiveDate::parse_from_str(&s(a, "date")?, "%Y-%m-%d")?;
            let start = NaiveTime::parse_from_str(&s(a, "start")?, "%H:%M")?;
            let end = NaiveTime::parse_from_str(&s(a, "end")?, "%H:%M")?;
            if end <= start {
                bail!("end must be after start within the same date");
            }
            f["zt"] = json!(s(a, "name")?);
            f["p_date"] = json!(date.format("%Y%m%d").to_string());
            f["p_start_time"] = json!(start.format("%H:%M").to_string());
            f["p_end_time"] = json!(end.format("%H:%M").to_string());
        } else if op == "schedule.delete" {
            f["grrlID"] = json!(s(a, "id")?);
        } else {
            bail!("Unknown schedule operation");
        }
        let h = post(c, JXMH_URL, &f, true).await?;
        hidden(&h, "token").context(
            "Calendar mutation response missing form; result uncertain, refresh before retry",
        )?;
        Ok(
            json!({"submitted":true,"message":capture(&h,r#"showMsg\("(.+?)"\)"#).ok(),"verification":"Form response received. Refresh schedule to verify persisted change."}),
        )
    }
}
fn parse_jsonp(h: &str) -> Result<Value> {
    let h = h.trim();
    if !h.starts_with('m') {
        bail!("Schedule response is not expected JSONP");
    }
    let begin = h.find('[').context("Missing schedule array")?;
    let end = h.rfind(']').context("Missing schedule array end")?;
    serde_json::from_str(&h[begin..=end]).context("Invalid schedule JSONP")
}
fn parse_weeks(detail: &str) -> Result<Vec<u32>> {
    if detail.contains("单周") {
        return Ok((1..=16).step_by(2).collect());
    }
    if detail.contains("双周") {
        return Ok((2..=16).step_by(2).collect());
    }
    if detail.contains("全周") {
        return Ok((1..=16).collect());
    }
    if detail.contains("前八周") || detail.contains("前8周") {
        return Ok((1..=8).collect());
    }
    if detail.contains("后八周") || detail.contains("后8周") {
        return Ok((9..=16).collect());
    }
    let pattern = capture(detail, r"(?:第|(?i:Week))([\d\-~,]+)")?.replace('~', "-");
    let mut out = vec![];
    for part in pattern.split(',') {
        let p = part.split('-').collect::<Vec<_>>();
        let start = p[0].parse::<u32>()?;
        let end = if p.len() == 1 {
            start
        } else if p.len() == 2 {
            p[1].parse()?
        } else {
            bail!("Bad week range")
        };
        if start < 1 || end < start || end > 60 {
            bail!("Bad week interval");
        }
        out.extend(start..=end);
    }
    out.sort();
    out.dedup();
    Ok(out)
}
fn parse_secondary(h: &str, first: &str) -> Result<(Vec<Value>, usize)> {
    use chrono::{Duration, NaiveDate};
    let first = NaiveDate::parse_from_str(first, "%Y-%m-%d")?;
    let re = Regex::new(
        r#"(?s)overlib\('(.+?)'\).*?>([^<>]+)</span>";\s*document.getElementById\('([^']+)'\)"#,
    )?;
    let begin = ["08:00", "09:50", "13:30", "15:20", "17:05", "19:20"];
    let end = ["09:35", "12:15", "15:05", "16:55", "18:40", "21:45"];
    let mut out = vec![];
    let mut bad = 0;
    for caps in re.captures_iter(h) {
        let detail = caps[1]
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        let basic = caps[3].as_bytes();
        let session = basic.get(1).and_then(|b| (*b as char).to_digit(10));
        let weekday = basic.get(3).and_then(|b| (*b as char).to_digit(10));
        let (Some(session), Some(weekday)) = (session, weekday) else {
            bad += 1;
            continue;
        };
        if !(1..=6).contains(&session) || !(1..=7).contains(&weekday) {
            bad += 1;
            continue;
        }
        let Ok(weeks) = parse_weeks(&detail) else {
            bad += 1;
            continue;
        };
        let location = capture(&detail, r"\(([^，]+)，").unwrap_or_default();
        for week in weeks {
            let date = first + Duration::days(((week - 1) * 7 + weekday - 1) as i64);
            out.push(json!({"name":&caps[2],"location":location,"date":date.to_string(),"start":begin[session as usize-1],"end":end[session as usize-1],"source":"secondary"}));
        }
    }
    let expected = h.matches("strHTML =").count();
    let actual = re.captures_iter(h).count();
    bad += expected.saturating_sub(actual);
    Ok((out, bad))
}
async fn news(session: &Session, op: &str, a: &Value) -> Result<Value> {
    let c = &session.client;
    let token = csrf(session).await?;
    let mut q = json!({"_csrf":token});
    let page = d(a, "page", "1");
    match op {
        "news.list" => {
            q["lmid"] = json!(d(a, "channel", "all"));
            q["currentPage"] = json!(page);
            q["length"] = json!(d(a, "length", "20"));
            jget(c, &query(NEWS_LIST_URL, &q)?).await
        }
        "news.search" => {
            let key = s(a, "keyword")?;
            let filter = if a["channel_name"].is_string() {
                json!({"lmmcgroup":s(a,"channel_name")?})
            } else {
                json!({})
            };
            let f = json!({"esParamClass":json!({"params":{"bt":key,"tag":key,"xxfl":key},"filterParams":filter,"orderMap":{"sort":"time"},"matchExact":if flag(a,"exact"){"是"}else{"否"},"currentPage":page}).to_string()});
            Ok(serde_json::from_str(
                &post(c, &query(SEARCH_NEWS_LIST_URL, &q)?, &f, false).await?,
            )?)
        }
        "news.detail" => news_detail(c, a, &token).await,
        "news.favorites" => Ok(serde_json::from_str(
            &post(
                c,
                &query(NEWS_FAVOR_LIST_URL, &q)?,
                &json!({"currentPage":page}),
                false,
            )
            .await?,
        )?),
        "news.favorite-add" | "news.favorite-remove" => {
            let u = format!(
                "{}{}",
                if op == "news.favorite-add" {
                    NEWS_ADD_FAVOR_URL
                } else {
                    NEWS_REMOVE_FAVOR_URL
                },
                encoded(&s(a, "id")?, false)
            );
            jget(c, &query(&u, &q)?).await
        }
        "news.subscriptions" => jget(c, &query(NEWS_SUBSCRIPTION_LIST_URL, &q)?).await,
        "news.sources" => {
            q["lmid"] = json!("");
            jget(c, &query(NEWS_SOURCE_LIST_URL, &q)?).await
        }
        "news.channels" => jget(c, &query(NEWS_CHANNEL_LIST_URL, &q)?).await,
        "news.subscription-add" => {
            if a["channel"].is_null() && a["source"].is_null() {
                bail!("channel or source is required");
            }
            let mut rule = json!({"bt":d(a,"keyword","")});
            if !a["channel"].is_null() {
                rule["lmid"] = a["channel"].clone();
            }
            if !a["source"].is_null() {
                rule["fbdwnm"] = a["source"].clone();
            }
            Ok(serde_json::from_str(
                &post(
                    c,
                    &query(NEWS_ADD_SUBSCRIPTION_URL, &q)?,
                    &json!({"dygz":rule.to_string(),"mkid":"XXFB"}),
                    false,
                )
                .await?,
            )?)
        }
        "news.subscription-remove" => {
            jget(
                c,
                &NEWS_REMOVE_SUBSCRIPTION_URL_FORMAT
                    .replace("{id}", &encoded(&s(a, "id")?, false))
                    .replace("{csrf}", &encoded(&token, false)),
            )
            .await
        }
        "news.subscribed" => Ok(serde_json::from_str(
            &post(
                c,
                &query(NEWS_LIST_BY_SUBSCRIPTION_URL, &q)?,
                &json!({"currentPage":page,"dyid":d(a,"id","")}),
                false,
            )
            .await?,
        )?),
        _ => bail!("Unknown news operation"),
    }
}
async fn gitlab(c: &Client, op: &str, a: &Value) -> Result<Value> {
    let page = d(a, "page", "1");
    let (path, q) = match op {
        "gitlab.namespaces" => ("/namespaces".to_owned(), json!({"page":page})),
        "gitlab.recent" => (
            "/projects".to_owned(),
            json!({"membership":true,"archived":false,"simple":true,"order_by":"last_activity_at","page":page}),
        ),
        "gitlab.personal" => (
            format!("/users/{}/projects", encoded(&s(a, "name")?, false)),
            json!({"simple":true,"order_by":"last_activity_at","page":page}),
        ),
        "gitlab.starred" => (
            "/projects".to_owned(),
            json!({"starred":true,"simple":true,"order_by":"last_activity_at","page":page}),
        ),
        "gitlab.search" => (
            "/search".to_owned(),
            json!({"scope":"projects","search":s(a,"query")?,"page":page}),
        ),
        "gitlab.project" => (
            format!("/projects/{}", encoded(&s(a, "id")?, false)),
            json!({}),
        ),
        "gitlab.tree" => (
            format!("/projects/{}/repository/tree", encoded(&s(a, "id")?, false)),
            json!({"path":d(a,"path",""),"ref":s(a,"ref")?,"page":page}),
        ),
        "gitlab.branches" => (
            format!(
                "/projects/{}/repository/branches",
                encoded(&s(a, "id")?, false)
            ),
            json!({}),
        ),
        "gitlab.blob" => {
            return Ok(
                json!({"content":get(c,&format!("{}/projects/{}/repository/blobs/{}/raw",GITLAB_API_BASE_URL,encoded(&s(a,"id")?,false),encoded(&s(a,"sha")?,false))).await?}),
            );
        }
        "gitlab.markdown" => {
            return jpost(
                c,
                &format!("{GITLAB_API_BASE_URL}/markdown"),
                &json!({"text":s(a,"text")?}),
            )
            .await;
        }
        _ => bail!("Unknown gitlab operation"),
    };
    jget(c, &query(&format!("{GITLAB_API_BASE_URL}{path}"), &q)?).await
}
async fn network(c: &Client, op: &str, a: &Value) -> Result<Value> {
    let home = get(c, NETWORK_HOME_URL).await?;
    if home.contains("loginform-verifycode") {
        bail!("Network service needs SSO login");
    }
    let doc = Html::parse_document(&home);
    match op {
"network.devices"=>Ok(json!(doc.select(&sel("#w1-container table tbody tr")).map(|e|{let t=cells(e);json!({"id":e.value().attr("data-key"),"ipv4":field_value(&t,0),"ipv6":field_value(&t,1),"loggedAt":field_value(&t,2),"permission":field_value(&t,3),"mac":field_value(&t,4)})}).collect::<Vec<_>>())),
"network.balance"=>{let t=doc.select(&sel("#w3-container table tbody tr td")).map(text).collect::<Vec<_>>();if t.len()<5{bail!("Network balance fields absent");}Ok(json!({"productName":t[0],"usedBytes":t[1],"usedSeconds":t[2],"accountBalance":t[3],"settlementDate":t[4]}))},
"network.account"=>{let h=get(c,NETWORK_USER_INFO_URL).await?;let t=Html::parse_document(&h).select(&sel("#w0 td")).map(text).collect::<Vec<_>>();let allowed=get(c,NETWORK_ALLOWED_DEVICES_URL).await?;let dt=Html::parse_document(&allowed);let allowed_text=dt.select(&sel(".glyphicon-exclamation-sign")).next().and_then(|e|e.parent().and_then(scraper::ElementRef::wrap)).map(text).unwrap_or_default();Ok(json!({"username":field_value(&t,0),"contactEmail":field_value(&t,1),"contactPhone":field_value(&t,2),"contactLandline":field_value(&t,5),"realName":field_value(&t,6),"userGroup":field_value(&t,7),"location":field_value(&t,3),"allowedDevices":capture(&allowed_text,r"(\d+)").ok(),"status":doc.select(&sel(".glyphicon-info-sign")).next().and_then(|e|e.parent().and_then(scraper::ElementRef::wrap)).map(text)}))},
"network.logout"=>{let token=hidden(&home,"_csrf-8800")?;let u=NETWORK_HOME_DELETE_URL.replace("{id}",&encoded(&s(a,"id")?,false)).replace("{mac}",&encoded(&s(a,"mac")?,false));let h=post(c,&u,&json!({"_csrf-8800":token}),false).await?;if !h.contains("w5-success-0"){bail!("Network disconnect not confirmed; inspect network.devices before retry");}Ok(json!({"disconnected":true}))},
"network.login"=>{let h=get(c,NETWORK_IMPORT_DEVICE_URL).await?;let ip=s(a,"ip")?;ip.parse::<std::net::IpAddr>().context("Invalid IP address")?;let h=post(c,NETWORK_IMPORT_DEVICE_URL,&json!({"_csrf-8800":hidden(&h,"_csrf-8800")?,"CertificationForm[ip]":ip,"CertificationForm[password]":s(a,"password")?,"CertificationForm[type]":if flag(a,"internet"){"out"}else{"in"}}),false).await?;let doc=Html::parse_document(&h);if let Some(e)=doc.select(&sel("#w0-success-0")).next(){Ok(json!({"message":text(e)}))}else{bail!("Network login rejected: {}",doc.select(&sel("#w0-error-0")).next().map(text).unwrap_or_else(||"Unrecognized response".to_owned()))}},
_=>bail!("Unknown network operation")}
}
async fn thos(c: &Client, op: &str, a: &Value) -> Result<Value> {
    const BASE:&str="https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421e4ff4e8f69247b59700f81b9991b2631ca359dd4";
    if op == "thos.score" {
        return jpost(c,THOS_SELECT_ONE_URL,&json!({"presetKey":"103765749452800","param":{"XH":s(a,"user_id")?,"KCH":s(a,"course_id")?}})).await;
    }
    if op == "thos.counts" {
        let v = jpost(c, &format!("{BASE}/fp/fp/formHome/allNum"), &json!({})).await?;
        if v["zbNum"].is_null() || v["AuditSvsNum"].is_null() {
            bail!("THOS count fields absent; not an empty result");
        }
        return Ok(v);
    }
    let kind = if op == "thos.services" {
        "services".to_owned()
    } else {
        s(a, "kind")?
    };
    let (path, keys) = match kind.as_str() {
        "services" => (
            "/fp/fp/formHome/AllSvsByConditionpage",
            vec!["project_id", "category_ids", "firstCharacter", "unit_id"],
        ),
        "active" => (
            "/fp/fp/myserviceapply/getZBSXList",
            vec![
                "serviceName",
                "startTime",
                "endTime",
                "procinst_id",
                "summary",
                "currentNode",
            ],
        ),
        "todo" => (
            "/fp/fp/taskcenter/getDBSXList",
            vec![
                "service_name",
                "job_number",
                "unit_name",
                "procinst_id",
                "currentNode",
                "summary",
                "start_date",
                "end_date",
            ],
        ),
        "completed" => (
            "/fp/fp/myserviceapply/getBJSXList",
            vec![
                "serviceName",
                "startTime",
                "endTime",
                "assess",
                "result",
                "completestart_time",
                "completeend_time",
                "procinst_id",
                "summary",
            ],
        ),
        "drafts" => (
            "/fp/fp/draft/pageDraft",
            vec!["draftName", "processInstId", "draftSummary"],
        ),
        "unread" => (
            "/fp/fp/carboncopy/getDYSXList",
            vec![
                "service_name",
                "start_date",
                "end_date",
                "apply_name",
                "unit_name",
                "procinst_id",
                "summary",
                "readresult",
                "bjstart_date",
                "bjend_date",
                "result",
            ],
        ),
        "phases" => (
            "/fp/fp/aggregation/getAggItemList",
            vec!["name", "agg_proc_id", "state"],
        ),
        _ => bail!("kind must be active,todo,completed,drafts,unread,phases"),
    };
    let mut v = json!({"pageNum":d(a,"page","1").parse::<u32>()?,"pageSize":d(a,"size","50").parse::<u32>()?});
    for k in keys {
        v[k] = json!("");
    }
    if kind == "todo" {
        v["status"] = json!("1");
    }
    if kind == "services" {
        v["orderBy"] = json!("defaultAsc");
        v["isCollect"] = json!("all");
        v["isRecommend"] = json!("no");
    }
    if a["keyword"].is_string() {
        v[if kind == "todo" || kind == "unread" {
            "service_name"
        } else {
            "serviceName"
        }] = a["keyword"].clone();
    }
    let mut result = jpost(c, &format!("{BASE}{path}"), &v).await?;
    if !result["list"].is_array() {
        bail!("THOS paginated response missing list; not treated as empty");
    }
    if kind == "phases" {
        if let Some(items) = result["list"].as_array_mut() {
            for item in items {
                let id = item["AGG_PROC_ID"]
                    .as_str()
                    .or(item["agg_proc_id"].as_str())
                    .map(str::to_owned);
                if let Some(id) = id {
                    item["phaseSteps"] = jpost(
                        c,
                        &format!("{BASE}/fp/aggregation/getActWork"),
                        &json!({"agg_proc_id":id}),
                    )
                    .await?;
                }
            }
        }
    }
    Ok(result)
}
async fn send_mail(c: &Client, a: &Value) -> Result<Value> {
    let recipient = s(a, "recipient")?;
    if recipient.contains(['\r', '\n', '"', '<', '>', ',', ';']) || !recipient.contains('@') {
        bail!("recipient must be a single email address");
    }
    let h = get(c, EMAIL_BASE_URL).await?;
    let doc = Html::parse_document(&h);
    let href = doc
        .select(&sel(".compose"))
        .next()
        .and_then(|e| e.value().attr("href"))
        .context("Coremail compose link missing; SSO session needed")?;
    let base = reqwest::Url::parse(EMAIL_BASE_URL)?;
    let compose = base.join(href)?;
    if compose.host_str() != base.host_str() || !compose.path().starts_with(base.path()) {
        bail!("Unexpected Coremail compose URL");
    }
    let h = get(c, compose.as_str()).await?;
    let doc = Html::parse_document(&h);
    let f = doc
        .select(&sel("#sendmail"))
        .next()
        .context("Coremail send form missing")?;
    let action = f.value().attr("action").context("Missing form action")?;
    let mut values = serde_json::Map::new();
    for e in f.select(&sel("input[name]")) {
        values.insert(
            e.value().attr("name").unwrap_or("").to_owned(),
            json!(e.value().attr("value").unwrap_or("")),
        );
    }
    for (k, v) in [
        ("subject", s(a, "subject")?),
        ("content", s(a, "content")?),
        ("to", format!("\"\"<{recipient}>,")),
        ("chkSaveToSent", "on".to_owned()),
        ("btnAddAttach", "0".to_owned()),
        ("btnCreateImg", "0".to_owned()),
        ("signSet", "-1".to_owned()),
    ] {
        values.insert(k.to_owned(), json!(v));
    }
    for k in [
        "chkUrgent",
        "chkNeedRcpt",
        "autoDel",
        "encryptPassword",
        "savePassword",
    ] {
        values.remove(k);
    }
    let url =
        format!("{EMAIL_BASE_URL}compose/{action}&action=deliver&needAudit=undefined&smsAddrs=");
    let h = post(c, &url, &Value::Object(values), false).await?;
    if !h.contains("savercpt.jsp") {
        bail!("Mail submission response uncertain. Check Sent before retrying; do not auto-resend");
    }
    Ok(json!({"submitted":true,"recipient":recipient}))
}
fn payment_form(h: &str, selector: &str) -> Result<(String, Value)> {
    let doc = Html::parse_document(h);
    let f = doc
        .select(&sel(selector))
        .next()
        .context("Payment form missing")?;
    let action = f
        .value()
        .attr("action")
        .context("Payment form action missing")?
        .to_owned();
    let url = reqwest::Url::parse(&action)?;
    let host = url.host_str().unwrap_or("");
    if url.scheme() != "https"
        || !(host == "webvpn.tsinghua.edu.cn"
            || host == "tsinghua.edu.cn"
            || host.ends_with(".tsinghua.edu.cn")
            || host == "alipay.com"
            || host.ends_with(".alipay.com"))
    {
        bail!("Unexpected payment destination; no request sent");
    }
    let mut v = serde_json::Map::new();
    for e in f.select(&sel("input[name]")) {
        v.insert(
            e.value().attr("name").unwrap_or("").to_owned(),
            json!(e.value().attr("value").unwrap_or("")),
        );
    }
    Ok((action, Value::Object(v)))
}
async fn recharge_dorm(c: &Client, a: &Value) -> Result<Value> {
    let amount = s(a, "amount")?.parse::<f64>()?;
    if !amount.is_finite() || amount <= 0.0 {
        bail!("amount must be positive");
    }
    let h = get(c, RECHARGE_ELE_URL).await?;
    let f = json!({"__EVENTTARGET":"","__EVENTARGUMENT":"","__VIEWSTATE":hidden(&h,"__VIEWSTATE")?,"__VIEWSTATEGENERATOR":hidden(&h,"__VIEWSTATEGENERATOR")?,"recharge_eleCtrl1$RadioButtonList1":"支付宝支付","write_money":amount,"username":hidden(&h,"username")?,"louhao":hidden(&h,"louhao")?,"room":hidden(&h,"room")?,"student_id":hidden(&h,"student_id")?,"banktype":"alipay"});
    let h = post(c, RECHARGE_PAY_ELE_URL, &f, true).await?;
    let (u, f) = payment_form(&h, "#banksubmit")?;
    let h = post(c, &u, &f, false).await?;
    let (u, f) = payment_form(&h, "form")?;
    let biz = f["biz_content"]
        .as_str()
        .context("Payment biz_content missing")?;
    let h = post(c, &u, &json!({"biz_content":biz}), false).await?;
    let code = hidden(&h, "qrCode")?;
    Ok(
        json!({"payment_url":code,"amount":amount,"paid":false,"next":"Review and complete payment manually in the payment app"}),
    )
}
async fn model_chat(c: &Client, a: &Value) -> Result<Value> {
    const BASE:&str="https://webvpn.tsinghua.edu.cn/https/77726476706e69737468656265737421fdf6459128346d5c300b9ae28c462a3b27469fc32211fa26a3e464";
    let messages = a["messages"]
        .as_array()
        .context("messages must be an array")?;
    if messages.is_empty() {
        bail!("messages cannot be empty");
    }
    let h = crate::auth::roam(
        c,
        &crate::auth::RoamOptions {
            policy: "default".to_owned(),
            payload: "19D04E39D96B36C494F2E48A1A4741FD".to_owned(),
            credentials: None,
        },
    )
    .await?;
    let token: Value = serde_json::from_str(&h)?;
    let token = token["data"].as_str().context("MadModel token absent")?;
    let mut payload = json!({"model":d(a,"model","DeepSeek-V4-Flash"),"messages":messages,"temperature":a.get("temperature").cloned().unwrap_or(json!(0.6)),"repetition_penalty":1.05,"stream":false});
    for k in ["tools", "max_tokens"] {
        if !a[k].is_null() {
            payload[k] = a[k].clone();
        }
    }
    let response = body(
        c.post(format!("{BASE}/v1/chat/completions"))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await?,
    )
    .await?;
    let v: Value = serde_json::from_str(&response)?;
    if !v["error"].is_null() {
        bail!("MadModel rejected request: {}", v["error"]);
    }
    Ok(v)
}
async fn news_detail(c: &Client, a: &Value, token: &str) -> Result<Value> {
    if let Ok(id) = s(a, "id") {
        return jget(
            c,
            &query(
                NEWS_DETAIL_URL,
                &json!({"xxid":id,"preview":"","_csrf":token}),
            )?,
        )
        .await;
    }
    let raw = s(a, "url")?;
    let url = if raw.starts_with('/') {
        format!("{NEWS_REDIRECT_URL}{raw}")
    } else {
        raw
    };
    let url = reqwest::Url::parse(&url)?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|h| h == "tsinghua.edu.cn" || h.ends_with(".tsinghua.edu.cn"))
    {
        bail!("News URL must be a Tsinghua HTTPS URL");
    }
    let r = c.get(url).send().await?.error_for_status()?;
    let final_url = r.url().to_string();
    let is_pdf = r
        .headers()
        .get("content-type")
        .and_then(|x| x.to_str().ok())
        .is_some_and(|x| x.contains("pdf"))
        || final_url.ends_with(".pdf");
    if is_pdf {
        use base64::Engine;
        return Ok(
            json!({"type":"pdf","encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(r.bytes().await?)}),
        );
    }
    let h = body(r).await?;
    if let Ok(id) = capture(&h, r#"var xxid\s*=\s*"(.*?)";"#) {
        return jget(
            c,
            &query(
                NEWS_DETAIL_URL,
                &json!({"xxid":id,"preview":"","_csrf":token}),
            )?,
        )
        .await;
    }
    if h.contains("_playFile") {
        if let Some(id) = reqwest::Url::parse(&final_url)?
            .query_pairs()
            .find(|(k, _)| k == "fileId")
            .map(|(_, v)| v.into_owned())
        {
            use base64::Engine;
            let r = c
                .get(query(
                    &format!("{PDF_NEWS_PREFIX}{}", encoded(&id, false)),
                    &json!({"_csrf":token}),
                )?)
                .send()
                .await?
                .error_for_status()?;
            return Ok(
                json!({"type":"pdf","encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(r.bytes().await?)}),
            );
        }
    }
    if final_url.contains("e3f5468534367f1e6d119aafd641303ceb8f9190006d6afc78336870/#/publish") {
        let id = final_url
            .rsplit('/')
            .next()
            .context("Publication ID absent")?;
        return jget(c, &format!("{SYSC_PDF_NEWS_PREFIX}{}", encoded(id, false))).await;
    }
    let doc = Html::parse_document(&h);
    Ok(
        json!({"type":"html","title":doc.select(&sel("h1,h2,h3,title")).next().map(text).unwrap_or_default(),"html":h,"text":doc.root_element().text().collect::<String>(),"normalization":"legacy source HTML retained; site-specific mobile layout not reproduced"}),
    )
}
fn parse_cr_weeks(pattern: &str, count: u32) -> Result<Vec<u32>> {
    if count == 0 || count > 60 {
        bail!("weeks must be 1..60");
    }
    let p = pattern.replace(['第', '周'], "");
    let out = match p.as_str() {
        "全" => (1..=count).collect(),
        "单" => (1..=count).filter(|w| w % 2 == 1).collect(),
        "双" => (1..=count).filter(|w| w % 2 == 0).collect(),
        "前八" | "前8" => (1..=8.min(count)).collect(),
        "后八" | "后8" => (count.saturating_sub(7).max(1)..=count).collect(),
        _ => parse_weeks(&format!("第{p}周"))?,
    };
    Ok(out)
}
fn parse_cr_schedule(h: &str, first: &str, weeks: u32) -> Result<(Vec<Value>, usize)> {
    use chrono::{Duration, NaiveDate};
    let first = NaiveDate::parse_from_str(first, "%Y-%m-%d")?;
    let start = h
        .find("function setInitValue")
        .context("CR timetable function missing")?;
    let end = h[start..]
        .find("initTopLocal")
        .map(|x| start + x)
        .context("CR timetable ending missing")?;
    let body = &h[start..end];
    let re = Regex::new(
        r#"(?s)strHTML\s*=\s*"";\s+var strHTML1\s*=\s*"";(.*?)getElementById\('a(\d+)_(\d+)'\)"#,
    )?;
    let fields_re = Regex::new(r#"strHTML1\s*\+=\s*["']；(.+?)["']"#)?;
    let begin = ["08:00", "09:50", "13:30", "15:20", "17:05", "19:20"];
    let finish = ["09:35", "12:15", "15:05", "16:55", "18:40", "21:45"];
    let mut out = vec![];
    let mut bad = 0;
    for cap in re.captures_iter(body) {
        let block = &cap[1];
        let session = cap[2].parse::<usize>()?;
        let day = cap[3].parse::<u32>()?;
        let fields = fields_re
            .captures_iter(block)
            .map(|x| x[1].to_owned())
            .collect::<Vec<_>>();
        let Ok(name) = capture(block, r"<b>(.+?)</b>") else {
            bad += 1;
            continue;
        };
        if fields.len() < 3 || !(1..=6).contains(&session) || !(1..=7).contains(&day) {
            bad += 1;
            continue;
        }
        let Ok(ws) = parse_cr_weeks(&fields[2], weeks) else {
            bad += 1;
            continue;
        };
        for w in ws {
            let date = first + Duration::days(((w - 1) * 7 + day - 1) as i64);
            out.push(json!({"name":name,"category":fields[1],"location":fields.get(3).cloned().unwrap_or_default(),"date":date.to_string(),"start":begin[session-1],"end":finish[session-1],"source":"cr"}));
        }
    }
    let expected = body.matches("var strHTML1").count();
    bad += expected.saturating_sub(re.captures_iter(body).count());
    Ok((out, bad))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percent_encoding_gbk_preserves_course_names() {
        assert_eq!(encoded("中文", true), "%D6%D0%CE%C4");
        assert_eq!(encoded("a&b =", false), "a%26b%20%3D");
    }
    #[test]
    fn required_arguments_do_not_silently_default() {
        assert!(s(&json!({}), "semester").is_err());
        assert_eq!(s(&json!({"id":123}), "id").unwrap(), "123");
    }
    #[test]
    fn token_extraction_handles_html_entities() {
        assert_eq!(
            hidden("<input name='token' value='a&amp;b'>", "token").unwrap(),
            "a&b"
        );
        assert!(hidden("<html>login</html>", "token").is_err());
    }
    #[test]
    fn jsonp_parser_rejects_login_html() {
        let v = parse_jsonp("m([{\"nr\":\"编译原理\",\"nq\":\"2026-11-01\"}]);").unwrap();
        assert_eq!(v[0]["nr"], "编译原理");
        assert!(parse_jsonp("<html>login</html>").is_err());
    }
    #[test]
    fn semester_alignment_matches_upstream() {
        let v = parse_semester(
            &json!({"kssj":"2026-09-08","jssj":"2026-12-20","id":"2026-2027-1","xnxqmc":"秋季"}),
        )
        .unwrap();
        assert_eq!(v["first_day"], "2026-09-07");
        assert_eq!(v["weeks"], 15);
        let v =
            parse_semester(&json!({"kssj":"2026-09-05","jssj":"2026-12-20","id":"s","xnxqmc":"n"}))
                .unwrap();
        assert_eq!(v["first_day"], "2026-09-07");
    }
    #[test]
    fn week_ranges_are_bounded() {
        assert_eq!(parse_weeks("第1,3-5周").unwrap(), vec![1, 3, 4, 5]);
        assert_eq!(parse_weeks("前八周").unwrap().len(), 8);
        assert!(parse_weeks("第5-3周").is_err());
        assert!(parse_weeks("第1-9999999周").is_err());
    }
    #[test]
    fn secondary_script_expands_sessions() {
        let h = r#"strHTML = "<span onmouseover=\"return overlib('课程(六教，第一周第1-3周)');\" onmouseout='return nd();'>编译原理</span>";
 document.getElementById('x1y2').innerHTML += strHTML+"<br>";"#;
        let (v, bad) = parse_secondary(h, "2026-09-07").unwrap();
        assert_eq!(bad, 0);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0]["date"], "2026-09-08");
        assert_eq!(v[0]["start"], "08:00");
    }
    #[test]
    fn mutation_catalogue_includes_get_based_writes() {
        let ops = operations();
        for name in [
            "course.select",
            "course.delete",
            "course.will",
            "news.favorite-add",
            "news.subscription-remove",
            "info.language",
            "dorm.password",
            "network.logout",
            "mail.send",
            "schedule.create",
            "passkey.remove",
        ] {
            assert!(
                ops.as_array()
                    .unwrap()
                    .iter()
                    .any(|o| o["name"] == name && o["mutating"] == true),
                "{name}"
            );
        }
    }
    #[test]
    fn catalogue_names_unique() {
        let mut names = std::collections::HashSet::new();
        for o in operations().as_array().unwrap() {
            assert!(names.insert(o["name"].as_str().unwrap().to_owned()));
        }
    }
    #[test]
    fn course_result_parser_preserves_response_message() {
        assert_eq!(
            message(r#"<script>showMsg("提交选课成功;");</script>"#).unwrap()["message"],
            "提交选课成功;"
        );
        assert!(message("error without message").is_err());
    }
    #[test]
    fn footer_pagination_is_not_invented() {
        let h = "<p class='yeM'>第 2 页 / 共 3 页（共 45 条记录）</p>";
        assert_eq!(
            footer(h),
            json!({"page":2,"total_pages":3,"total_count":45})
        );
        assert!(footer("<html>unexpected</html>")["total_count"].is_null());
    }
    #[test]
    fn current_course_catalog_keeps_teacher_and_detail_identity() {
        let h = r#"
<table><tr class="trr2">
  <td>计算机系</td><td>40250001</td><td>01</td><td><a href="js.vjsKcbBs.do?m=showToXs&p_id=T0001%3B40250001">课程</a></td>
  <td>3</td><td><a href="showJsDetail.do?p_jsh=T0001">教师</a></td>
  <td>40</td><td>5</td><td>10</td><td>4</td><td>1-2</td><td>说明</td><td>特色</td><td>2</td><td>否</td><td></td><td>否</td><td>否</td><td>文化</td>
</tr></table>"#;
        let rows = course_catalog_rows(h);
        assert_eq!(rows[0]["teacherId"], "T0001");
        assert_eq!(rows[0]["bksRemaining"], "5");
        assert_eq!(rows[0]["yjsRemaining"], "4");
        assert!(rows[0]["detailUrl"].as_str().unwrap().contains("showToXs"));
    }
    #[test]
    fn course_level_rows_use_the_live_type_map() {
        let h = r#"
<table><tr class="trr2"><td>1</td><td>40250001</td><td>01</td><td>必修</td></tr>
<tr class="trr2"><td>2</td><td>40750002</td><td>02</td><td></td></tr></table>"#;
        assert_eq!(
            course_level_rows(h),
            json!([
                {"id":"40250001","seq":"1","property":"必修","typeCode":"006","typeLabel":"必修"},
                {"id":"40750002","seq":"2","property":"","typeCode":"ty","typeLabel":"体育"}
            ])
        );
    }
    #[test]
    fn selected_courses_tolerate_shifting_columns_and_normalize_will() {
        let h = r#"
<script>var zy=[["40250001,01","2","008","0","否"]];</script>
<table><tr class="trr2">
  <td>必修</td><td>第一志愿</td>
  <td><input name="p_del_id" value="2026-2027-1;40250001;01;"></td>
  <td>40250001</td><td>课程名</td><td>01</td><td>1-2(全周)</td><td>教师</td><td>3</td><td>否</td>
</tr><tr class="trr2">
  <td>不可退</td><td>必修</td><td>40470169</td><td>计算机科学研究实践</td><td>0</td>
  <td>(全周)</td><td>陈一镭</td><td>9</td><td>否</td><td></td>
</tr></table>"#;
        let rows = selected_course_rows(h).unwrap();
        assert_eq!(rows[0]["id"], "40250001");
        assert_eq!(rows[0]["seq"], "01");
        assert_eq!(rows[0]["will"], 2);
        assert_eq!(rows[0]["type"], "限选");
        assert_eq!(rows[0]["name"], "课程名");
        assert_eq!(rows[0]["teacher"], "教师");
        assert_eq!(rows[1]["id"], "40470169");
        assert_eq!(rows[1]["seq"], "0");
        assert_eq!(rows[1]["name"], "计算机科学研究实践");
        assert_eq!(rows[1]["time"], "(全周)");
        assert_eq!(rows[1]["teacher"], "陈一镭");
        assert_eq!(rows[1]["credit"], "9");
    }
    #[test]
    fn whole_timetable_merges_repeated_course_slots() {
        let h = r#"
<script>p_id=123;40250001; var strHTML1=""; strHTML1 += "；张老师"; strHTML1 += "；限选"; strHTML1 += "；1-2周";
document.getElementById('a1_2').innerHTML += "<b>编译原理</b>";</script>
<script>p_id=123;40250001; var strHTML1=""; strHTML1 += "；张老师"; strHTML1 += "；限选"; strHTML1 += "；1-2周";
document.getElementById('a2_2').innerHTML += "<b>编译原理</b>";</script>"#;
        let rows = whole_timetable_rows(h).unwrap();
        assert_eq!(rows[0]["id"], "40250001");
        assert_eq!(rows[0]["teacher"], "张老师");
        assert_eq!(rows[0]["type"], "限选");
        assert_eq!(rows[0]["time"], "2-1(1-2周),2-2(1-2周)");
    }
    #[test]
    fn one_thu_course_operations_are_in_the_catalog() {
        let operations = operations();
        for name in [
            "course.selected",
            "course.select",
            "course.delete",
            "course.will",
            "course.queue",
            "course.priority",
            "course.levels",
            "course.detail",
            "course.whole-schedule",
        ] {
            assert!(
                operations
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|operation| operation["name"] == name),
                "{name}"
            );
        }
        let delete = operations
            .as_array()
            .unwrap()
            .iter()
            .find(|operation| operation["name"] == "course.delete")
            .unwrap();
        assert!(delete["optional"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "candidate"));
    }
    #[tokio::test]
    async fn gbk_form_request_contract() {
        use wiremock::matchers::{body_string, header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let srv = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/form"))
            .and(header("content-type", "application/x-www-form-urlencoded"))
            .and(body_string("name=%D6%D0%CE%C4&token=a%26b"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(1)
            .mount(&srv)
            .await;
        let c = Client::new();
        assert_eq!(
            post(
                &c,
                &format!("{}/form", srv.uri()),
                &json!({"name":"中文","token":"a&b"}),
                true
            )
            .await
            .unwrap(),
            "ok"
        );
    }
    #[tokio::test]
    async fn auth_failure_html_is_not_success() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let srv = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("用户登陆超时或访问内容不存在"),
            )
            .mount(&srv)
            .await;
        assert!(get(&Client::new(), &srv.uri()).await.is_err());
    }
    #[test]
    fn summer_cr_schedule_expands_week_patterns() {
        let h = r#"function setInitValue(){ strHTML = ""; var strHTML1 = "";
strHTML += "<b>编译原理</b>";
strHTML1 += "；张老师";
strHTML1 += "；必修";
strHTML1 += "；第1-2周";
strHTML1 += "；六教";
getElementById('a2_3').innerHTML += strHTML; } initTopLocal"#;
        let (items, bad) = parse_cr_schedule(h, "2026-09-07", 8).unwrap();
        assert_eq!(bad, 0);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["date"], "2026-09-09");
        assert_eq!(items[0]["start"], "09:50");
        assert_eq!(items[1]["date"], "2026-09-16");
    }
    #[test]
    fn summer_weeks_use_actual_semester_length() {
        assert_eq!(parse_cr_weeks("全周", 3).unwrap(), vec![1, 2, 3]);
        assert_eq!(
            parse_cr_weeks("后八周", 10).unwrap(),
            vec![3, 4, 5, 6, 7, 8, 9, 10]
        );
        assert!(parse_cr_weeks("全周", 999).is_err());
        assert_eq!(parse_weeks("week1-2").unwrap(), vec![1, 2]);
    }
    #[test]
    fn payment_form_rejects_unrelated_destination() {
        assert!(payment_form(
            "<form action='https://example.com/pay'><input name='secret' value='x'></form>",
            "form"
        )
        .is_err());
        let (url,v)=payment_form("<form action='https://openapi.alipay.com/gateway.do'><input name='biz_content' value='value'></form>","form").unwrap();
        assert_eq!(url, "https://openapi.alipay.com/gateway.do");
        assert_eq!(v["biz_content"], "value");
    }
}
