# Upstream provenance and license notices

Source snapshots inspected on 2026-10-30:

| Source | Revision | Scope used |
|---|---|---|
| https://github.com/thu-info-community/thu-info-app | `7930cad3f0d7243ad54df3ce4443b911113eef91` | `packages/thu-info-lib` protocols, constants, parsers; selected app-only network services |
| https://github.com/robertying/learnX | `e6d81fbeb619b0837263688839b8e01c5c664ebe` | Dependency selection, CourseX/update integration, app functionality inventory |
| https://github.com/robertying/thu-learn-lib | `1b79b984f34a6f1ca0532beaf59abac55db7f52c` | Learn native protocol implementation, matching learnX lockfile |

This is a protocol/client port, not a packaged mobile UI. The upstream mobile
binaries and JavaScript runtime are not redistributed. Nevertheless, translated
code can carry upstream licensing obligations; the notices are retained rather
than assuming a new language removes them.

## THU Info

Both the root notice and `@thu-info/lib` package identify BSL (Business Source License 1.1). The root notice names UNIDY2002 as licensor, states **Additional Use Grant: None**, and says the Change Date is four years from first release of the specific version, with MIT as the Change License. The exact notice is in `licenses/thu-info-app-LICENSE`.

BSL is not an unrestricted current MIT grant. Do not assume arbitrary production
deployment, redistribution, or commercial service use is authorized. Public
source distribution does not change the upstream restrictions or create an
Additional Use Grant. Determine how the upstream terms apply to your intended
deployment and obtain permission where required. This package does not assert a
legal determination that every source-oriented or local use is prohibited or
permitted.

## learnX and thu-learn-lib

learnX's notice is labelled MIT but adds a restriction for people who work or have worked at Tsinghua's Computer and Information Managing Center, or projects financially supported by institutions related to Tsinghua. Its exact terms are retained in `licenses/learnX-LICENSE` / `learnX-MIT.txt`.

The used `thu-learn-lib` dependency has a conventional MIT notice, retained in `licenses/thu-learn-lib-MIT.txt`. This is separate from the app notice and should not be conflated with it.

## Native dependencies and additions

`Cargo.lock` pins the Rust dependencies. Their own license metadata and notices
remain applicable. Rustls performs TLS validation; SM2 is implemented with
libsm. A historical Linux static build used musl and its notice remains included.
No claim is made that the entire combined package can be relicensed as MIT.
Original standalone glue is offered under the MIT notice included in
`licenses/original-glue-MIT.txt`, subject to existing rights in translated
portions.

This document explains packaging provenance and cautions; it is not a substitute for the source terms or legal advice.
