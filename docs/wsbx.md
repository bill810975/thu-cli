# Tsinghua reimbursement

The `wsbx.*` operations implement the Tsinghua online reimbursement service at
`wsbx.tsinghua.edu.cn`. The CLI keeps the current web app's route-specific
contracts instead of forcing every receipt through one daily-expense payload.

Receipt-policy routing is separate from the API mechanics. Use
`skills/thu-reimbursement` to classify the document, collect the required
approval/evidence, and decide whether a foreign payment still needs an external
foreign-exchange step.

## Supported routes

| `ywlx` | Use | Save/submit endpoint |
| --- | --- | --- |
| `052` | Daily reimbursement | `pay/whole/submit` |
| `051` | Smart reimbursement, including a foreign `INVOICE`/`RECEIPT` already paid personally | `pay/whole/submit` |
| `06` | Domestic travel reimbursement | `clf/gn/tmp/save` |
| `19` | Conference reimbursement | `pay/whole/submit` |
| `1206` | International-travel reimbursement with separate RMB and foreign-currency rows | `intl/travel/save` |

The route table and exact endpoints are available offline:

```sh
thu-cli --compact thu wsbx.routes
```

An international conference is not one universal WSBX form:

- A foreign registration/publication invoice already paid by an individual
  normally belongs in the general/smart `051` route.
- A reimbursement tied to an approved international trip starts in `1206`,
  which reads and preserves the approved RMB/foreign-currency budget rows.
- A Tsinghua-hosted conference may use the `19` conference route for eligible
  domestic expenses. International approval and separate conference budget
  evidence still apply.

Choose the route from the approved activity and Finance's live business-type
list, not only from the seller's country.

## Authentication

The client uses the authenticated Info profile to request a short-lived wsbx SSO
ticket. It exchanges that ticket for the wsbx API token, then updates the token
from each successful response. The token is stored in a private mode-600 sidecar
beside the Info session file; it is never printed.

No browser Cookie extraction, background process, or cron keepalive is required.
A read-only request may obtain a fresh Info ticket and retry once. A write is
never replayed automatically because its outcome may already be committed.

## Discover the live account

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.user

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.business-types

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"ywlx":"1206","page_size":20}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"ywlx":"06","status":"ALL-2"}'
```

Use operation discovery for the live argument names:

```sh
thu-cli describe thu wsbx.draft.save
thu-cli describe thu wsbx.intl.budget
```

The default route remains `052` for backward compatibility. Pass `ywlx`
explicitly for every new workflow.

## Add receipts to the wallet

An invoice must be in `我的票夹` before it can be bound to a reimbursement.
The CLI now performs the same local-file ingestion path without requiring the
web app:

```sh
# OCR and inspect the normalized result without changing the wallet
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.identify \
  --args '{"file":"/absolute/path/to/invoice.pdf"}'

# OCR, verify, and add it to the wallet
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.add \
  --args '{"file":"/absolute/path/to/invoice.pdf"}' --confirm

# Search the wallet after ingestion
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.wallet.list \
  --args '{"query":"INVOICE_NUMBER","status":"pending"}'
```

`invoice.identify` accepts a regular, non-symlink PDF, OFD, image, or ZIP file
up to 30 MiB. `invoice.add` runs OCR, normalizes the verification fields,
checks for an exact existing invoice number, and then calls
`invoice/verify/save`. A repeated add returns the existing wallet record instead
of blindly inserting it again. The result separates `added`, `existing`, and
`failed` documents and sets `complete` only when no document failed.

Some departments require a witness when the handler, project owner, and invoice
handler are the same person. WSBX then rejects binding with its business error.
Search the employee directory and add the witness before retrying the bind:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.employee.search --args '{"query":"EMPLOYEE_NAME_OR_ID"}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.invoice.add-witness \
  --args-file PRIVATE_WITNESS_ARGS.json --confirm
```

The witness argument file contains `invoice_numbers`, `witness`,
`witness_name`, and an accurate business `note`. Camera capture and mobile QR
upload are still outside the CLI; provide the downloaded source file instead.

## Preview a route payload offline

`wsbx.draft.preview` builds and checks the route-specific shape without
authentication or network access. It also enforces that an externally supplied
`payload.ywbh` matches the top-level `ywbh` and normalizes `saveType`.

```sh
thu-cli --compact thu wsbx.draft.preview \
  --args-file PRIVATE_REIMBURSEMENT_ARGS.json
```

The same argument file can be used with `wsbx.draft.save` or
`wsbx.draft.submit`. `--dry-run` checks only the outer operation contract;
`wsbx.draft.preview` checks the actual route payload.

## Common draft sequence

1. Classify the receipt, then use `wsbx.invoice.identify` and
   `wsbx.invoice.add` to put it in the wallet when it is not already there.
2. Add a required witness with `wsbx.employee.search` and
   `wsbx.invoice.add-witness` before reporting an unexplained bind rejection.
3. Find the exact `bmbh`/`xmbh` in `wsbx.project.list`.
4. Search for an existing draft with `wsbx.draft.list`.
5. If a new draft is authorized, run `wsbx.draft.create` with the chosen
   `ywlx`.
6. Bind the project with `wsbx.draft.bind-project`.
7. Read the live route template:

   ```sh
   thu-cli ... thu wsbx.draft.template \
     --args-file PRIVATE_TEMPLATE_ARGS.json
   ```

   The operation dispatches to:

   - `052`: `zdhb/template/get`
   - `051` and `19`: `common/templateInfo/get`
   - `06`: `common/tempInfo`
   - `1206`: `intl/travel/init`

8. Bind unused wallet invoices with `wsbx.invoice.bind`.
9. Read and upload the required files with `wsbx.attachment.types`,
   `wsbx.attachment.list`, and `wsbx.attachment.upload`.
10. Allocate the reimbursement amount to a payment method. For a daily expense
    already paid personally, use `"self_payment": true`; if the employee has
    multiple bank accounts, use
    `"self_payment": {"account": "gzkh1", "note": "报销款"}`.
11. Save with `wsbx.draft.save`.
12. Inspect with `wsbx.draft.get`, `wsbx.draft.status`, and
    `wsbx.payment.init`.
13. Submit separately only after explicit authorization for the exact receipt,
    project, amount, expense items, attachments, and payment rows.

Invoice binding is route-aware:

- `052` uses `zdhb/dzfp/bind` and assigns the requested expense item directly.
- Other routes use the shared `invoice/binding` contract.
- `19` additionally assigns conference expense categories.
- `06`, `051`, and `1206` may require route-specific travel/allowance/currency
  rows after invoice selection.

Unbinding a conference invoice can require several linked cleanup requests.
Never blindly retry an ambiguous unbind or bind.

## Daily reimbursement: `052`

Argument essentials:

```json
{
  "ywbh": "BUSINESS_NUMBER",
  "ywlx": "052",
  "project": {"bmbh": "DEPARTMENT", "xmbh": "PROJECT"},
  "phone": "CONTACT_PHONE",
  "purpose": "SHORT_PURPOSE",
  "description": "OPTIONAL_DETAIL",
  "items": [],
  "payments": {},
  "is_online": true,
  "no_paper": true
}
```

For domestic e-invoice daily reimbursement (`052`), `is_online=true` and
`no_paper=true` are the CLI defaults. Submission then uses workflow state `8`
(online push). Pass either value as `false` only for an explicitly different
approved workflow; the builder also accepts the strings `"1"` and `"0"`.

Daily submission also requires payment allocation. The explicit `payments`
object remains available for repayment, card, private-payee, public-transfer,
and project-transfer rows. For the current employee's own bank account,
`self_payment: true` builds a campus private-payment row using the unique
available account. If more than one account exists, select one of `yhzh`,
`gzkh1` through `gzkh5`, or `gwkh`:

```json
{
  "self_payment": {
    "account": "gzkh1",
    "note": "报销款"
  }
}
```

The CLI fetches the employee bank data, masks it from normal status output, and
requires the payment rows to total exactly the reimbursement item total before
submitting. Use `wsbx.payment.init` to inspect saved payment rows after a save
and `wsbx.payment.methods` to inspect methods enabled for the route.

Each selected `items` row carries the live template's `jjflkmbh`,
`jjflkmmc`, electronic-invoice amount/count fields, and paper-receipt
amount/count fields. Payment arrays are `cjkList`, `gwkList`, `xmzzList`,
`dszzList`, and `dgzzList`.

## Smart or foreign invoice: `051`

Use this route for a foreign `INVOICE`/`RECEIPT` already paid personally in RMB
when Finance permits the expense under smart reimbursement.

Argument essentials:

```json
{
  "ywbh": "BUSINESS_NUMBER",
  "ywlx": "051",
  "project": {"bmbh": "DEPARTMENT", "xmbh": "PROJECT"},
  "phone": "CONTACT_PHONE",
  "base_info": {"bxsy": "SHORT_PURPOSE", "bxqksm": "DETAIL"},
  "invoice_list": [],
  "items": [],
  "paper_items": [],
  "payments": {}
}
```

The operation writes the web app's `invoiceList`, `bxmbInfoList`,
`ybbxBaseInfo`, payment arrays, and `czr` fields. Keep the foreign invoice,
RMB card/bank record, approval, and any translation/annotation requirement
together. The CLI does not convert the foreign amount to RMB; use the amount and
payment evidence accepted by Finance.

## Domestic travel: `06`

The domestic-travel endpoint is different from `pay/whole/submit`.

```json
{
  "ywbh": "BUSINESS_NUMBER",
  "ywlx": "06",
  "project": {"bmbh": "DEPARTMENT", "xmbh": "PROJECT"},
  "phone": "CONTACT_PHONE",
  "travel": {
    "cllx": "TRAVEL_TYPE",
    "ccsy": "BUSINESS_PURPOSE",
    "ccrq": ["YYYY-MM-DD", "YYYY-MM-DD"],
    "ts": "DAYS",
    "ccdd": "DESTINATION",
    "cczmrbh": "DEFAULT_TRAVELER_ID",
    "cczmrmc": "DEFAULT_TRAVELER_NAME",
    "invoiceList": [],
    "fpjInfoList": [],
    "bzList": []
  },
  "base_info": {
    "bmbh": "DEPARTMENT",
    "xmbh": "PROJECT"
  },
  "payments": {}
}
```

The CLI preserves the web app's `ywbxrBase`, `baseInfoObj`, invoice list,
paper-expense list, special explanation list, and payment arrays. Travel dates,
train/flight tickets, accommodation evidence, itinerary, meeting notice, and
payment records still have to satisfy the Finance rules.

## Conference: `19`

```json
{
  "ywbh": "BUSINESS_NUMBER",
  "ywlx": "19",
  "project": {"bmbh": "DEPARTMENT", "xmbh": "PROJECT"},
  "phone": "CONTACT_PHONE",
  "conference": {
    "hyfBaseInfo": {
      "hymc": "MEETING_NAME",
      "hydd": "LOCATION",
      "hylb": "MEETING_TYPE",
      "hyxs": "FORMAT",
      "hykssj": "YYYY-MM-DD HH:mm:ss",
      "hyjssj": "YYYY-MM-DD HH:mm:ss",
      "ysbh": "APPROVED_BUDGET_NUMBER"
    },
    "hyfZcmxList": []
  },
  "payments": {}
}
```

Useful conference reads:

```sh
thu-cli ... thu wsbx.conference.budgets --args-file PRIVATE_BUDGET_QUERY.json
thu-cli ... thu wsbx.conference.expenses --args-file PRIVATE_EXPENSE_QUERY.json
```

The conference route calculates people/days, approved budget categories,
online/offline meeting costs, and paper expense rows. Do not submit until the
meeting approval, budget, notice, attendance evidence, and itemized cost data
agree.

## International travel: `1206`

Start from the approved international-travel budget rather than inventing
foreign-currency rows.

```sh
# Query by authorization code
thu-cli ... thu wsbx.intl.budget \
  --args '{"author_code":"AUTHORIZATION_CODE"}'

# Or query by approval number and traveler
thu-cli ... thu wsbx.intl.budget \
  --args '{"approval_number":"APPROVAL_NUMBER","employee":"EMPLOYEE_ID"}'

thu-cli ... thu wsbx.currency.list
thu-cli ... thu wsbx.currency.rate --args '{"currency":"CURRENCY_ID"}'
```

The budget response contains the approved people, regions, RMB/foreign-currency
items, rates, loan data, and budget identifiers. Pass that response as
`intl_budget`:

```json
{
  "ywbh": "BUSINESS_NUMBER",
  "ywlx": "1206",
  "project": {"bmbh": "DEPARTMENT", "xmbh": "PROJECT"},
  "intl_budget": {"data": {}},
  "actual_line": "ACTUAL_ROUTE",
  "actual_start_date": "YYYY-MM-DD",
  "actual_end_date": "YYYY-MM-DD",
  "payments": {}
}
```

Before saving, verify the returned `base`, `ry`, `wb`, `wbDetail`, `ftfy`,
`currencyRate`, `reexchange`, `conversion`, and payment object. Additional
international operations are:

```sh
thu-cli ... thu wsbx.intl.get --args '{"ywbh":"BUSINESS_NUMBER"}'
thu-cli ... thu wsbx.intl.currency.split --args-file PRIVATE_SPLIT_ARGS.json
thu-cli ... thu wsbx.intl.invoices --args '{"ywbh":"BUSINESS_NUMBER"}'
thu-cli ... thu wsbx.intl.invoice.update --args-file PRIVATE_INVOICE_ARGS.json
thu-cli ... thu wsbx.intl.files --args '{"serial_number":"FILE_SERIAL"}'
```

`intl.currency.split` is read-only on WSBX and returns proposed per-person
currency rows; it does not perform a purchase or bank transfer.

## Exact route payload

When an existing draft has route fields that are not represented by the compact
arguments above, pass a server-shaped object as `payload`:

```json
{
  "ywbh": "BUSINESS_NUMBER",
  "ywlx": "19",
  "project": {"bmbh": "DEPARTMENT", "xmbh": "PROJECT"},
  "payload": {}
}
```

The builder requires `payload.ywbh` to equal the top-level `ywbh`, forces
`saveType` to `"0"` for save and `"1"` for submit, and fills route-specific
guard fields only when absent. Preview the merged result before a write.

## Mutation and submission boundaries

- Every create/bind/upload/save/submit/delete/revoke operation is marked
  `mutating:true` and requires `--confirm`.
- `wsbx.draft.submit` creates a real approval request. It requires explicit
  authorization for the exact route and receipt.
- A timed-out write may already have reached WSBX. Inspect draft, invoice, file,
  and workflow state before retrying; mutations are never retried automatically.
- `wsbx.draft.delete` permanently deletes a draft and is not an undo operation.
- The WSBX client does not solve CAPTCHAs, sign approval decisions, capture a
  receipt through a mobile camera/QR flow, or make a bank/card payment.
- International Office approval, exchange-budget approval, foreign-contract tax
  filing, Bank of China pre-review, foreign exchange, and remittance are separate
  external workflows. A saved `1206` draft is not proof that a university
  remittance has been paid.
- Keep private argument files at mode `600` and remove them after use.

The `052` project-binding/draft-save path, local receipt OCR/verification,
wallet insertion, witness assignment, and invoice binding have been checked
against a live account. The `051`, `06`, `19`, and `1206` contracts are
reconstructed from the current WSBX frontend and covered by offline fixtures;
live account verification remains an explicit release boundary.
