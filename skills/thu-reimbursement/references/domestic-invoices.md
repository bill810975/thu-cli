# Domestic Receipt Reimbursement

This reference reflects the Finance Office's September 2026 rules. Verify unusual
or high-value cases with the responsible Finance Office team.

## Accepted Domestic Receipt Types

- Tax invoices: VAT special/common invoices, digital invoices, railway and air
  transport electronic invoices, general machine-printed invoices, quota
  invoices, and industry invoices.
- Fiscal receipts: non-tax revenue, administrative fund settlement, donation,
  social-organization membership, and medical receipts.
- Travel and other lawful external receipts such as train tickets, airline
  itineraries, embassy fees, and bank fees.
- Tsinghua internal settlement receipts for electricity, water, and internal
  transfers.

## Validity Checks

- Tax invoices must name buyer `清华大学` and use taxpayer ID
  `12100000400000624D`.
- Paper invoices and non-fully-digital electronic invoices should contain an
  invoice special seal or valid electronic signature. Fully digital invoices
  rely on the PDF/OFD electronic signature and require the source file.
- The invoice must be genuine, complete, legible, unchanged, and tied to the
  actual university business.
- Generic descriptions such as `办公用品`, `材料`, `住宿费`, or `图书` require an
  itemized purchase list when the invoice itself is not sufficiently detailed.
- A single paper receipt, or consecutive receipts for one expense whose total is
  at least CNY 5,000, must be authenticity-checked. `我的票夹` is preferred.
- Official verification entry points include
  <https://inv-veri.chinatax.gov.cn/> for tax invoices and
  <http://pjcy.mof.gov.cn/#/home> for national electronic fiscal receipts.
- A single economic transaction of CNY 1,000 or more must use a non-cash payment
  method such as bank transfer, card, university card, or internal transfer.
  Keep the payment record when an individual paid first.
- Reimbursement must be submitted no later than the last workday of March in the
  following year.

## Signatures and Form

- The operator and project owner/PI must sign or approve. Electronic approval in
  the reimbursement system is acceptable where supported.
- If the PI is also the expense handler and department rules require financial
  supervisor approval, get the required additional approval.
- Use the business-specific reimbursement form, not only the invoice. Examples
  include domestic travel, international travel, conference, procurement, and
  labor templates.

## Common Expense Evidence

| Expense | Additional evidence |
| --- | --- |
| Office supplies, materials, devices | Itemized list; asset registration when the asset threshold applies; contract/procurement approval when required |
| Domestic travel | Approved trip/business purpose, tickets, accommodation invoice with guest/days/unit price or hotel folio, meeting notice, and payment records |
| Domestic conference hosted by Tsinghua | Meeting notice, approved budget, attendance list or online participant count, and itemized meeting-cost list if the invoice says `会议费` |
| Tsinghua-hosted international conference | International conference approval, separately approved international conference budget, notice, attendance evidence, and domestic or foreign invoices as applicable |
| Publication fee | Fee/payment notice, invoice, and contract when the contract threshold applies |
| Test/processing service | Invoice, itemized service list, contract when required; participant compensation uses bank payroll |
| Labor or consulting | Use the university bank-payroll system and the relevant agreement/meeting evidence; do not use a substitute invoice to pay an individual |
| Purchase/lease/construction | Current contract, procurement, asset-registration, approval, and acceptance documents required by the specific category |

## CLI Draft Sequence

Use `wsbx=052` only for the daily-reimbursement path, `051` for an eligible smart
form, `06` for domestic travel, `19` for conference reimbursement, and `1206`
for an approved international-travel activity. The CLI dispatches template,
invoice, draft, and submit behavior by `ywlx`; do not force an unrelated expense
into `052`. Domestic e-invoice daily reimbursement defaults to paperless
(`no_paper=true`) and online push (`is_online=true`).

1. `wsbx.project.list` selects the exact `bmbh` and `xmbh`.
2. `wsbx.draft.list` checks for an existing draft; otherwise create one only
   when the user has authorized the draft mutation.
3. `wsbx.invoice.identify` checks OCR; authorized `wsbx.invoice.add` verifies and
   inserts a local PDF/OFD/image source into `我的票夹`; an exact duplicate is
   returned as `existing`.
4. `wsbx.draft.bind-project` returns the live item template.
5. `wsbx.invoice.wallet.list` confirms the pending/used wallet state, and
   `wsbx.invoice.list status=pending` checks the electronic-invoice pool.
6. `wsbx.invoice.bind` binds the selected wallet invoice to a valid item.
   If binding reports same-person witness validation, use
   `wsbx.employee.search` and authorized `wsbx.invoice.add-witness` with an
   accurate note before retrying.
7. `wsbx.attachment.types` and `wsbx.attachment.list` determine required files.
8. `wsbx.attachment.upload` adds supporting evidence.
9. `wsbx.draft.preview` validates the complete route payload offline, then
   `wsbx.draft.save` saves the draft.
10. `wsbx.draft.get` and `wsbx.draft.status` are the final inspection before any
   separately authorized `wsbx.draft.submit`.

## Stop Conditions

- Invoice buyer or taxpayer ID is wrong.
- The receipt appears used, duplicated, altered, or unrelated to the project.
- The amount exceeds CNY 1,000 and no non-cash payment evidence is available.
- The relevant contract, procurement approval, asset registration, travel
  approval, conference approval, or itemized list is missing.
- The expense category or `ywlx` is ambiguous.
