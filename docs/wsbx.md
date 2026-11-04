# Online reimbursement

The `wsbx.*` operations implement the Tsinghua online reimbursement service
(`wsbx.tsinghua.edu.cn`). The completed end-to-end path is the current daily
reimbursement workflow, `ywlx=052`.

## Authentication

The client uses the authenticated Info profile to request a short-lived wsbx SSO
ticket. It exchanges that ticket for the wsbx API token, then updates the token
from each successful response. The token is stored in a private sidecar beside
the session file, for example `info.wsbx-auth`, with mode `600`.

No browser Cookie extraction, background process or cron keepalive is required.
When the API token expires, a read-only operation obtains a fresh Info ticket and
retries once. A write operation is never replayed after the request may have
reached the server.

## Discover

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.user

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.business-types

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.project.list --args '{"page_size":20}'

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu wsbx.draft.list --args '{"status":"ALL-2"}'
```

Use the live catalog for exact argument metadata:

```sh
thu-cli services thu
thu-cli describe thu wsbx.draft.save
```

## Build a daily reimbursement draft

1. Allocate a business number:

   ```sh
   thu-cli ... thu wsbx.draft.create --confirm
   ```

2. Select one project object from `wsbx.project.list` and bind it:

   ```sh
   thu-cli ... thu wsbx.draft.bind-project \
     --args-file PRIVATE_REIMBURSEMENT_ARGS.json --confirm
   ```

   The result contains the available expense-item template. Keep the returned
   `jjflkmbh` and `jjflkmmc` on each item.

3. Find pending invoices and bind them to the relevant expense item:

   ```sh
   thu-cli ... thu wsbx.invoice.list \
     --args '{"ywbh":"BUSINESS_NUMBER"}'

   thu-cli ... thu wsbx.invoice.bind \
     --args-file PRIVATE_REIMBURSEMENT_ARGS.json --confirm
   ```

   Pass `"status":"used"` to search reimbursement-in-progress and completed
   invoices instead. This is the reliable way to check whether a specific
   invoice was already consumed and to retrieve the business number shown by
   the service. `"status":"pending"` is the default.

4. Upload any required files using an attachment type returned by
   `wsbx.attachment.types`:

   ```sh
   thu-cli ... thu wsbx.attachment.upload \
     --args-file PRIVATE_REIMBURSEMENT_ARGS.json --confirm
   ```

5. Save the draft:

   ```sh
   thu-cli ... thu wsbx.draft.save \
     --args-file PRIVATE_REIMBURSEMENT_ARGS.json --confirm
   ```

`wsbx.draft.save` requires `ywbh`, `project`, `phone`, and `items`. Optional
fields include `purpose`, `description`, `payments`, and `no_paper`. Payment
rows use the service arrays `cjkList`, `gwkList`, `xmzzList`, `dszzList`, and
`dgzzList`. An empty `items` array is allowed for an incomplete draft.

## Submit

First inspect the exact draft and workflow state:

```sh
thu-cli ... thu wsbx.draft.get --args '{"ywbh":"BUSINESS_NUMBER"}'
thu-cli ... thu wsbx.draft.status --args '{"ywbh":"BUSINESS_NUMBER"}'
```

Then use the same validated arguments with the submission operation:

```sh
thu-cli ... thu wsbx.draft.submit \
  --args-file PRIVATE_REIMBURSEMENT_ARGS.json --confirm
```

The operation performs one submit request and never retries it automatically.
If the result is ambiguous, inspect `wsbx.draft.status` before doing anything
else.

## Mutation boundaries

- Project binding, invoice binding/unbinding, attachment changes, save, submit,
  delete and withdrawal are marked mutating and require `--confirm`.
- `wsbx.draft.delete` permanently deletes a receipt. It is not an undo operation.
- `wsbx.draft.submit` creates a real approval request. Use it only for the exact
  receipt and fields the user authorized.
- The client does not sign approval decisions, solve CAPTCHAs, perform invoice
  OCR, execute mobile QR uploads, or complete a separate payment/wallet flow.
- Private argument files should use mode `600` and should be removed after use,
  especially when they contain invoice identifiers, bank details, or payees.
