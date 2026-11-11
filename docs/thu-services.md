# Info service catalogue and interface discovery

The Info service portal publishes application cards from:

```text
GET /b/yyfw/vyyfwxx/info/portal_fg/common/getCyAppList/xsAll
```

`service.categories`, `service.list`, and `service.audit` read that live
catalogue instead of maintaining a stale hand-written list. Each application
has a stable `yyfwid`, category, display name, and an annotation when an
existing native adapter covers part of the service.

## Operations

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  thu service.categories

thu-cli --session "$HOME/.config/thu-cli/info.json" \
  thu service.list --args '{"category":"学习","page_size":100}'

thu-cli --session "$HOME/.config/thu-cli/info.json" \
  thu service.info --args '{"yyfwid":"YYFWID"}'

thu-cli --session "$HOME/.config/thu-cli/info.json" \
  thu service.resolve --args '{"yyfwid":"YYFWID"}'

thu-cli --session "$HOME/.config/thu-cli/info.json" \
  thu service.discover --args '{"yyfwid":"YYFWID","max_scripts":12}'
```

`service.audit` applies the same metadata and SSO resolution flow to every
matching card. Add `"deep":true` to fetch landing pages and a bounded number of
static JavaScript bundles. The bundle walker follows module and dynamically
referenced JavaScript assets discovered inside the initial bundles. Filters
include `category`, `query`, `native_only`, and `limit`; `concurrency` is capped
at 12.

```sh
thu-cli --session "$HOME/.config/thu-cli/info.json" \
  thu service.audit \
  --args '{"deep":true,"max_scripts":8,"concurrency":8}'
```

## Safety boundary

Discovery never invokes an endpoint found inside an application's HTML or
JavaScript. It performs only portal catalogue/metadata reads, `onlineAppRedirect`
resolution, the official SSO landing navigation, and GET requests for static
script assets. Raw ticket-bearing URLs and query values are omitted from output.

Extracted paths are normalized to `key=<value>` query summaries and classified
as `read_candidate`, `write_candidate`, or `unclassified_candidate`.
Classification is conservative and is not a guarantee that every candidate is
callable by the current account.

## Live audit snapshot

The full read-only audit on 2026-11-10 covered all 306 cards in 14 categories.
Of those, 268 resolved to a usable landing destination and 81 yielded endpoint
candidates. The static pass reported 1,157 candidate records across the 306
applications, representing 1,004 distinct normalized paths: 121 read candidates,
481 conservative write-risk candidates, and 555 candidates whose HTTP method
could not be inferred statically. The remaining 38 cards did not expose a
usable destination to the current student session, commonly because of role
restrictions, HTTP
401/403/404/500, an expired application session, or an SSO response without a
roaming URL.

The portal's `getYyfwjs` metadata endpoint is recorded and attempted, but the
current student profile received an error result for every card. Catalogue,
SSO, and static discovery remained available.

This static method does not execute application JavaScript. SPAs that construct
API paths dynamically after client-side routing can still hide endpoints; run
`service.discover` again with a larger `max_scripts` when reviewing one of those
applications.
