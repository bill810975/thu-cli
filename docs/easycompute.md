# EasyCompute and Paratera

The EasyCompute portal at `https://easycompute.cs.tsinghua.edu.cn` authenticates
through Tsinghua unified identity and exposes the Paratera control plane for
machines, API keys, images, zones, and pricing.

The CLI keeps the EasyCompute frontend token, one-time Tsinghua access code,
Paratera session token, and account metadata in the private mode-600
`easycompute-auth` sidecar beside the selected Info profile. Tokens are never
printed. Logout, cookie import, or a profile identity change removes the
sidecar.

## Discovery

Start with read-only calls:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu easycompute.account

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu easycompute.clusters

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu paratera.session

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu paratera.api-keys

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu paratera.containers

thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu paratera.container.options
```

`paratera.container.options` returns the current zones, GPU offerings,
resource types, public image types, and private-image payload. The optional
`availability_payload`, `spec_payload`, and `image_payload` arguments let the
caller repeat the corresponding frontend queries with an exact filter.
`easycompute.clusters` returns a compact list of the cluster IDs/names and
zones exposed by the current Paratera control plane, including the IDs used by
container planning.

## LLM API keys

`paratera.api-keys` returns key metadata only. Create, enable/disable, rename,
and delete require `--confirm`. Key creation writes the complete one-time
provider response to a new mode-600 file instead of printing key material:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu paratera.api-key.create \
  --args '{"token_name":"my-key","output":"/absolute/private/path/key.json"}' \
  --confirm
```

The route set is `/lms/api/key/{llmSk,create,update,delete}`. Paratera access
keys use `/platform/cipher/{queryCiphers,createCipher,updateCipher,deleteCipher}`.

## Container machines

Build the server-shaped request offline first:

```sh
thu-cli --compact thu paratera.container.plan \
  --args '{
    "alias_name":"research-5090",
    "cluster_id":105,
    "region_id":104,
    "zone_id":160,
    "service_model":"rtx5090.xeon6530.xlarge",
    "image_uuid":"ackci-0000qxv64",
    "billing_type":"PostPaid",
    "count":1
  }'
```

Then check the exact configuration:

```sh
thu-cli --compact --session "$HOME/.config/thu-cli/info.json" \
  thu paratera.container.preflight \
  --args '{"payload":{...}}'
```

Only after reviewing the returned price, availability, region, GPU model,
image, disk, and billing period should `paratera.container.create` be repeated
with the same payload and `--confirm`.

The create route is `/platform/ack/service/create`. The current frontend also
uses `ack/.../getUserResourceContainerServiceDetailList`,
`resourcePrice/checkContainerPrice`, and
`resourceContainerService/describeServiceAvailableResource`.

No live container or API key was created during this integration.

## Known upstream behavior

- The current EasyCompute landing-page bundle still contains a cluster-request
  modal that calls `GET /auth/cluster/info` and `POST /auth/cluster/apply`.
  Both routes return HTTP 404, and the modal is not reachable from the live
  authenticated landing page. The CLI therefore uses the working Paratera zone
  APIs for `easycompute.clusters` and does not expose the dead write route.
- A 5090 preflight returned a successful price quote and a current sold-out
  availability result. Availability is account-, zone-, and time-dependent.
- `paratera.read` is restricted to an allowlist of known read endpoints and
  refuses state-changing methods.
