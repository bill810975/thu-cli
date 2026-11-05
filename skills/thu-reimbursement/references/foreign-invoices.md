# Foreign Receipt Reimbursement

The 2026 Finance Office guide treats foreign receipts as other lawful external
receipts, commonly marked `INVOICE` or `RECEIPT`. Determine how the expense was
or will be paid before choosing the reimbursement workflow.

Treat Hong Kong, Macau, and Taiwan official-travel or conference cases under the
same `出国（境）` approval and evidence framework, even when a receipt is written in
Chinese or the payment was made in RMB.

Foreign and paper receipts are not automatically present in the domestic
electronic-invoice pool used by `wsbx.invoice.list`. If the invoice cannot be
found there, do not conclude that it is invalid; use the verified source file and
the applicable attachment/foreign-payment process.

## Decision Tree

### 1. Already paid by an individual in RMB

- Use the valid foreign invoice/receipt plus the RMB card or bank statement.
- The official guide states that a personal credit-card foreign payment cannot
  reimburse foreign currency, but the foreign invoice plus the card-payment
  record may support reimbursement of the equivalent RMB when the expense is
  otherwise compliant.
- For overseas travel, also follow the approved trip and travel evidence rules.
- For foreign invoices sent by email or fax, the project owner must sign before
  reimbursement.

### 2. University must pay a foreign supplier or institution

- Use the foreign-exchange workflow; do not submit this as an ordinary `052`
  draft that ignores currency and remittance approval.
- Complete an exchange application with the project/department code, foreign
  currency amount, detailed purpose, payment responsibility for foreign-bank
  charges, PI signature, and department seal.
- For `会议费` or `版面费`, attach the invoice/payment notice.
- For `服务费`, education, or research cooperation, attach the invoice/payment
  notice and contract. If the total contract value exceeds the equivalent of
  USD 50,000, complete tax filing with the fund/salary office before payment.
- Foreign-exchange applications equivalent to USD 50,000 or more, except
  overseas travel cash withdrawals, require Finance Office leadership approval.
  Below that threshold, the Accounting Center foreign-exchange counter handles
  the application.
- For a dual-non purchase, use the Bank of China online pre-review flow:
  fill the form and upload complete supporting materials, pass pre-review,
  obtain department/finance review, then submit the paper materials to the
  foreign-exchange desk.
- Contracts or payment notices supplied electronically or by fax must carry the
  department seal before bank payment.

### 3. Official overseas travel

Required evidence includes:

- `出国/赴港澳任务批件` or the applicable Taiwan approval/record.
- Approved exchange budget for trips under three months or the long-term form
  for trips of three months or more.
- `清华大学因公出国（境）报销收支情况表`.
- International tickets, registration, accommodation, city transport, local
  transfer, visa, insurance, and other claimed-receipt originals.
- Passport/identity-document, visa/permit, and entry/exit evidence; a boarding
  pass or immigration record may substitute for a missing entry stamp.
- Orders, hotel folios, car-rental details, ride records, insurance policy, and
  payment records as applicable.

Every foreign-travel receipt must have Chinese annotations for content, date,
place, quantity, and amount, and must be signed by the traveler. City-to-city
transport receipts also need Chinese origin/destination labels. International
air travel is generally limited to standard economy class. Meals and incidentals
are per diem items where the rules provide for them, not invoice items.

### 4. Online attendance at an overseas conference

- Approval must be obtained before participation when the conference is held
  overseas or involves participants from outside mainland China, Hong Kong,
  Macau, or Taiwan.
- Use the Tsinghua Online Service System international cooperation route.
- Keep the approval, conference notice containing the registration amount,
  foreign invoice/receipt, and payment evidence.

### 5. Foreign publication or conference registration fee

- Foreign publication invoices normally use `INVOICE` or `RECEIPT` and the payer
  should identify `Tsinghua University`.
- Keep the fee notice, invoice/receipt, and any required contract.
- International conference registration also requires the meeting invitation or
  notice and, for official travel or online attendance, the applicable approval.
- If the university will remit the fee, complete the foreign-exchange or
  dual-non purchase flow before payment.

### 6. Foreign visitor, labor fee, or airfare

Foreign personnel costs require the applicable International Office approval or
record and passport/visa evidence. Labor paid in foreign currency requires the
exchange application and the university labor-fee worksheet. A foreign expert's
airfare payment requires the invoice, boarding pass, invitation/contract, and
passport/visa evidence.

## Foreign Invoice Adequacy

- Treat `INVOICE`/`RECEIPT` as a valid category only when it is a genuine,
  complete, business-related original or accepted electronic source.
- Confirm seller, buyer/payee, date, description, currency, amount, tax where
  shown, invoice number, and payment status.
- A buyer name of `Tsinghua University` is explicitly required for foreign
  publication fees; check the payer name on other supplier documents.
- Email/fax foreign invoices require project-owner signature before domestic
  reimbursement.
- If the document is illegible, incomplete, altered, or the currency/payment
  evidence cannot be reconciled, stop and ask Finance.

## Payments and Bank Charges

- The university generally bears the domestic bank charge; the overseas
  beneficiary generally bears foreign/intermediary bank charges unless the
  supplier requires otherwise.
- If Tsinghua agrees to bear an overseas bank charge, select the applicable
  option on the exchange application and provide the expenditure project code.
- Exchange vouchers are valid only on the issue date. Bank processing and
  student/staff identity requirements must be checked with the foreign-exchange
  counter.

## CLI Boundary

`thu-cli` can inspect projects, drafts, invoices, and attachments. It supports:

- `051` smart reimbursement for a foreign invoice/receipt already paid by an
  individual when Finance permits that route.
- `1206` international-travel budget query, RMB/foreign-currency row split,
  invoice update, attachment read, draft save, and submit.
- `19` conference reimbursement for eligible Tsinghua-hosted conference costs.

These WSBX operations do not perform:

- International Office approval.
- Exchange budget approval.
- Tax filing for foreign contracts.
- Bank of China dual-non purchase pre-review.
- Foreign currency conversion or remittance.
- Completion or approval of a foreign-payment request.

Report the required external step instead of presenting a saved `051` or `1206`
draft, or a submitted WSBX approval, as a completed foreign-payment/remittance.
