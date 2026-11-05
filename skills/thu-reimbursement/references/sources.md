# Source Map

Verified on 2026-11-05 through Tsinghua Info.

## Primary Source

Finance Office notice:
[关于发布《清华大学会计核算与服务指南（2026年9月版）》和《清华大学财务报销规范（2026年9月版）》的通知](https://info.tsinghua.edu.cn/f/info/xxfb_fg/xnzx/template/detail?xxid=5cb6aaf3e3504b55b55e87f937096a56)

The notice attaches:

- `会计核算与服务指南（2026年9月）发布版.pdf`
- `清华大学财务报销规范（2026年9月）发布版.pdf`

## Reimbursement Rules

`清华大学财务报销规范（2026年9月）`:

- `1-1` to `1-4`, printed pages 1-3: receipt classes, foreign receipts,
  invoice validity, signatures, verification, non-cash settlement, deadline.
- `2-10`, printed pages 18-19: official international travel.
- `2-11`, printed page 20: short-term foreign visitor travel.
- `2-24`, printed pages 25-26: domestic and international conference costs.
- `2-28`, printed page 27: publication fees and foreign publication invoices.
- `2-36-2`, printed pages 29-30: foreign-person labor fees.
- Appendix 1, printed page 40: Tsinghua invoice header information.
- Appendix 2, printed pages 40-41: official invoice verification sites.

## Foreign Exchange

`会计核算与服务指南（2026年9月）`, section `三、外汇业务`, printed pages
91-101:

- Exchange application and Table 21 material matrix.
- USD 50,000 approval and contract/tax-filing rules.
- Foreign exchange account payment and Table 23 material matrix.
- Foreign invoice handling, international registration, and foreign airfare.
- Credit-card foreign payment settled in equivalent RMB.

## Reimbursement Interface Sources

Checked on 2026-11-05 from the current WSBX web bundle:

- Daily `052` route: `/zdhb`, `zdhb/template/get`, `pay/whole/submit`.
- Smart/general `051` route: `/rcbx`, `common/templateInfo/get`,
  `pay/whole/submit`, and `invoice/binding`.
- Domestic travel `06`: `/gnclfbx`, `common/tempInfo`,
  `clf/gn/tmp/save`.
- Conference `19`: `/hyfbx`, `hyf/gnzcmx/list`, `pay/whole/submit`.
- International travel `1206`: `/gjclxhbx`, `intl/travel/budget/list`,
  `intl/travel/init`, `intl/travel/wb/split`, and `intl/travel/save`.

The `smartThise/OneTHU` repository was also checked at commit
`38a3aa695d23fc51cc3cffd212862bb85c54d8a9`. It contains the separate electronic
invoice-list/PDF service and online-service navigation, but no additional WSBX
reimbursement microservice or draft/submit payload beyond the upstream invoice
client.

## Maintenance

These are operational interpretations of the September 2026 rules, not a
substitute for Finance Office review. If a newer guide, project-specific rule,
or written Finance Office instruction conflicts with this skill, follow the
newer written instruction and update this source map and both workflow
references.
