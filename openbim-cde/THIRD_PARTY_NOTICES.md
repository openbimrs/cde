# Third-party notices and source provenance

`openbim-cde` contains independently authored Rust wire models informed by
publicly available buildingSMART API descriptions. It does not include copies of
the upstream JSON/YAML files or locally licensed DIN documents.

## Publication status

The inspected Foundation API and Documents API repositories did not contain an
explicit license grant for their machine-readable contracts. Omitting those
files does not by itself resolve whether a translation of their interface
definitions requires permission. Therefore Cargo publication is disabled with
`publish = false` until the maintainers obtain written permission or legal
clearance.

This notice records provenance and attribution; it is not a license grant for
third-party material. The repository's MIT license applies only to original
OpenBIM.rs contributions to the extent those contributions are licensable by
its authors.

## Foundation API 1.1

- Source: <https://github.com/buildingSMART/foundation-API/tree/release_1_1>
- Commit: `2c552eae435d575f1df7c11ff9ea19e3d2a324ce`
- `schemas/auth_GET.json`: `28f6ece4cbb180450c35190ca91860209d0b73aaa446982bde0abbbf43dad906`
- `schemas/dynRegClient_GET.json`: `8cfaaa2e205a24cc64502627178304567849862146eab0a26d7e9030503b0968`
- `schemas/dynRegClient_POST.json`: `ba0b5e258bf1ff3d65e91162496ff2c80820fe60de655e982fbda4d03f34aba1`
- `schemas/error.json`: `cec6b1a36818b2b3c5bfc9030e440e5fec0f97cfea263e2f6cf8229b93f24b63`
- `schemas/user_GET.json`: `9fcacd7b5659863908edffb5a2f64a2929b32a7032db4413869e31cf7e55328e`
- `schemas/versions_GET.json`: `38e32d9cc2ce87eba72a583b6ef7fb18e5569191566727627dee9d77a842d38d`

## Documents API 1.0

- Source: <https://github.com/buildingSMART/documents-API/tree/release_1_0>
- Commit: `938bd7636e094c2dc3c65ad6dae846ac3c7b6a9b`
- `swagger.yaml`: `b6d5b5ee5da62648e385d5e468a9589f3ea4b385fabcfd3598b5a8bb668b6bbf`

## Historical research source

The archived umbrella repository was used only to establish that it is not the
current contract:

- Source: <https://github.com/buildingSMART/OpenCDE-API/tree/master>
- Commit: `13b359fbd6ac2299cc9a02ace1f9b67563715cd1`
- `openapi/openCDE-spec.yaml`: `4da013375e3edf45db1ed31b1c1fcb32f672ca939c56b6beb001f3f1972fd877`
