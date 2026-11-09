---
name: thu-reimbursement
description: Plan and execute Tsinghua University domestic and foreign receipt reimbursements with thu-cli, including document classification, foreign-payment routing, evidence checks, and explicit mutation safety.
---

# Tsinghua Reimbursement

Use this skill when preparing a Tsinghua reimbursement, deciding whether a
receipt needs domestic invoice handling or a foreign-payment workflow, or
checking whether an invoice was already used. Use the installed `thu-cli` for
account, project, draft, invoice, and attachment operations.

The authoritative baseline is the Finance Office's September 2026
`财务报销规范` and `会计核算与服务指南`. Re-check Info before a live submission
when the result is high-value, unusual, or near a policy threshold.

## First Classify the Receipt

1. Domestic tax invoice: VAT special/common invoice, digital electronic invoice,
   railway or air-ticket invoice, general machine-printed or quota invoice.
2. Domestic fiscal or internal receipt: non-tax revenue receipt, donation
   receipt, medical receipt, Tsinghua internal settlement voucher, or utility
   receipt.
3. Foreign receipt: a document headed `INVOICE` or `RECEIPT`, or another
   legally valid receipt issued outside the domestic tax/fiscal systems.
4. Payment route: already paid personally in RMB, paid by a university project,
   or still requires foreign currency/remittance.

Read [references/domestic-invoices.md](references/domestic-invoices.md) for the
domestic path. Read [references/foreign-invoices.md](references/foreign-invoices.md)
for foreign receipts, foreign travel, foreign registration/publication fees, or
cross-border payment. Read [references/sources.md](references/sources.md) when
checking the policy basis or updating the skill.

## Collect Before Mutating

Extract or ask for these facts:

- Exact business purpose and whether it is tied to the approved project.
- Project code and department code.
- PI/operator identity and the phone number used on the reimbursement.
- Invoice or receipt file, invoice number when present, seller, buyer, date,
  amount, tax, currency, and payment evidence.
- Expense category and whether a contract, approval, invitation, meeting
  notice, trip approval, or other supporting document is required.
- Whether this is a new reimbursement, an existing draft, or an invoice already
  consumed by an earlier reimbursement.
- Exact witness/proof person for every invoice (`zmr`/`zmrmc`).
- Reimbursement payee and bank/account choice: the operator or another named
  recipient.
- Exact funding project, including project number and name.
- Approval routing: submit with the operator as responsible person, or send the
  approval to a specifically named project owner/other approver.

Do not infer an ambiguous expense category merely from the seller's name. Use
the project's live reimbursement template or ask the user.

## Mandatory Reimbursement Setup Confirmation

Before the first effectful write that selects the witness, funding, payment
destination, or approval route, ask the user to confirm all four items below.
Read-only inspection may happen first. If an existing draft already contains
these values, read them back and ask the user to confirm or correct them.

1. **Invoice witness/proof person:** `发票证明人写谁？`
2. **Reimbursement payee:** `报销到谁的账上？` If not the operator, obtain the
   exact intended recipient and select only a live payment method that supports
   that recipient.
3. **Funding project:** `用哪笔经费？` Use the exact project number and project
   name; do not select a project from the most recent draft or available
   balance.
4. **Approval routing:** `要推送到别的经费负责人手上，还是直接填自己？`
   If routing to another person, confirm the exact project owner/approver.

Never default the witness to the PI, project owner, operator, or the person used
on earlier invoices. Prior reimbursements are evidence only; the user must
confirm the witness for each new reimbursement. Lack of an explicit answer
blocks the witness mutation and final submission.

Before `wsbx.draft.submit`, show a compact final confirmation containing the
witness, payee/account, project, and approval route, together with the invoice,
amount, expense category, and attachments. Require explicit approval of that
complete set.

## CLI-First Inspection

Use the authenticated Info profile with `thu-cli`:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" thu wsbx.user
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"page_size":20}'
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"status":"ALL-2"}'
```

For a domestic electronic invoice, first check whether its source file has
already been ingested into `我的票夹`. Use `invoice.identify` to inspect OCR and
`invoice.add --confirm` only after the user authorizes adding this exact file.
An exact invoice-number match in either wallet state makes `invoice.add`
idempotent and is returned as `existing`.

Then check both reimbursement-pool states when duplicate use is possible:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.list \
  --args '{"query":"INVOICE_NUMBER","status":"pending"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.list \
  --args '{"query":"INVOICE_NUMBER","status":"used"}'
```

An empty pending result is not enough to conclude that an invoice is new. A
used result identifies the reimbursement that consumed it. Inspect its
`ywbh`, then use `wsbx.draft.get` and `wsbx.draft.status` before taking action.

Read the CLI's route map when the form is not obvious:

```sh
thu-cli --compact thu wsbx.routes
```

Supported routes are daily `052`, smart/general `051`, domestic travel `06`,
conference `19`, and international travel `1206`. Choose from the approved
activity and Finance's live business type, not from the seller country alone.

For a normal domestic electronic invoice on `052`, paperless reimbursement and
online push are the default (`no_paper=true`, `is_online=true`). Override them
only when the user has confirmed that the applicable Finance workflow differs.

## Prepare the Correct Path

### Domestic invoice or general receipt

After adding the source file to the wallet and choosing an existing draft or
creating one:

1. Bind the exact project with `wsbx.draft.bind-project`.
2. Read the route template and choose a valid expense item. Use `052` for the
   daily form and `051` for an eligible smart form.
3. Bind unused wallet invoices with `wsbx.invoice.bind`.
4. If WSBX rejects binding because the handler, project owner, and invoice
   handler are the same person, search `wsbx.employee.search`, then use
   authorized `wsbx.invoice.add-witness` only after the user has supplied the
   witness required by the mandatory setup confirmation above.
5. Upload required supporting documents with
   `wsbx.attachment.upload`, using a category returned by
   `wsbx.attachment.types`.
6. Run `wsbx.draft.preview` against the exact draft payload.
7. Allocate the amount to payment. For a personally paid daily expense, set
   `self_payment:true`, or select a named account when multiple exist. The
   payment total must equal the reimbursement total.
8. Save with `wsbx.draft.save`.
9. Inspect `wsbx.payment.init` together with the draft before submission.

Paper, fiscal, internal, or non-pooled receipts usually need attachment upload
and the supporting evidence listed for that expense type.

### Domestic travel

Use `ywlx=06`. The travel fields are not a daily-expense payload: provide the
travel type, purpose, date range, days, destination, traveler, ticket invoice
list, paper expenses, special explanations, and payment rows. The CLI calls
`clf/gn/tmp/save`.

Check the approved trip/purpose, train or flight tickets, accommodation folio,
itinerary, meeting notice, and non-cash payment records before submission.

### Conference

Use `ywlx=19` for the conference form. Start with
`wsbx.conference.budgets`, then `wsbx.conference.expenses` after binding the
project. Confirm meeting type/format, dates, locations, delegate counts,
approved budget number, online/offline costs, people-days, and signed variance
explanations.

The conference form can include eligible domestic and overseas meeting costs,
but a separately approved international conference budget and any applicable
International Office approval are still required. Do not infer eligibility
from the form route.

### Foreign receipt already paid in RMB

Use `ywlx=051` when Finance permits the expense under smart reimbursement and
the individual has already paid. Keep the foreign invoice or receipt and the
RMB card/bank payment record together. For email/fax receipts, obtain the
required Chinese annotations and project-owner signature.

Do not create a `1206` international-travel draft merely because the document is
foreign. `1206` starts from an approved international-travel budget; `051` is
the ordinary foreign-invoice route when no international-travel budget/report is
needed.

### International travel or conference activity

Use `ywlx=1206` for reimbursement tied to an approved international trip or
international conference activity. Start with `wsbx.intl.budget` by
authorization code or approval number plus traveler. The approved result contains
the `base`, traveler, RMB/foreign-currency budget, rate, loan, and statement data
used by the live form.

- Review `wsbx.intl.get`, `wsbx.intl.invoices`, and `wsbx.intl.files`.
- Use `wsbx.intl.currency.split` to obtain WSBX's proposed per-person foreign
  currency rows; it does not purchase currency.
- Use `wsbx.intl.invoice.update` for the invoice reimbursement amount/category.
- Provide actual route and actual start/end dates before `wsbx.draft.submit`.
- Keep the approval, conference/travel notice, itinerary, tickets, invoices,
  card/bank records, exchange approval, and any conference specified-hotel
  evidence.

An international conference can therefore involve multiple linked paths: `1206`
for the approved trip/activity, `051` for an already-paid foreign invoice when
appropriate, and `19` only for eligible Tsinghua-hosted conference costs.

### Foreign receipt requiring payment or foreign exchange

Stop before claiming reimbursement is complete and follow
[references/foreign-invoices.md](references/foreign-invoices.md). This path can
require International Office approval, an exchange budget, a foreign exchange
application, online Bank of China pre-review, contracts, tax filing, and the
Accounting Center foreign-exchange counter.

The CLI supports WSBX `051` and `1206` draft save/submit, currency-budget reads,
and foreign-currency row handling. It does not perform International Office
approval, foreign-exchange application, Bank of China pre-review, purchase,
exchange, or remittance completion. A saved/submitted `1206` draft is not proof
that the university paid the overseas beneficiary.

## Mutation and Submission Rules

- A `mutating:true` operation changes campus data and requires `--confirm`.
- Run `thu-cli describe thu OPERATION` for the exact live argument contract.
- Use `--dry-run` before supported mutations when validating local inputs.
- Never retry a timed-out or ambiguous write automatically. Inspect the draft,
  invoice state, and workflow state first.
- `wsbx.draft.submit` creates a real approval. Use it only after the user
  explicitly authorizes the exact receipt, project, category, amount,
  attachments, and payment rows.
- Immediately before submission, read back and confirm the witness
  (`zmr`/`zmrmc`), reimbursement payee/account, funding project, and approval
  routing. Do not silently overwrite or "correct" any of these fields.
- For daily `052` and smart `051`, `zt=8` is still pending approval submission. The CLI's
  `draft.submit` completes both phases and must reach `zt=9` before reporting
  that the receipt entered approval.
- Never submit automatically because the invoice was recognized or because the
  draft saved successfully.
- Never expose cookies, tokens, full account records, bank data, or unrelated
  reimbursement history.
