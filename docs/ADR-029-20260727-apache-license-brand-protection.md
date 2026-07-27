# ADR-029: Apache 2.0 License with Brand Protection for Fibonsai and Cryptomeria

## Context

Cryptomeria was originally released as a proprietary project under "Fibonsai internal project" terms, with no formal license file. As the project matures and aims to attract external contributors, a standard open-source license is needed to clarify usage, modification, and distribution rights.

The chosen license must:
- Allow open collaboration and contribution from the community
- Protect the Fibonsai and Cryptomeria brands from misuse
- Be compatible with common open-source ecosystem practices

## Options Considered

1. **Proprietary (status quo)** — no formal license, internal-only
   - Blocks external contributions entirely
   - No legal clarity for users or contributors

2. **GNU General Public License v3 (GPL-3.0)** — strong copyleft
   - Requires derivative works to also be GPL-licensed
   - May deter commercial adoption and partnerships
   - No built-in brand protection

3. **GNU Lesser General Public License v3 (LGPL-3.0)** — weaker copyleft
   - Allows linking from non-GPL code
   - Still imposes copyleft on modifications to the library itself
   - No built-in brand protection

4. **Apache License 2.0** — permissive with patent grant
   - Allows commercial use, modification, and distribution
   - Includes an express patent grant from contributors
   - Standard in data infrastructure and Rust ecosystem
   - No built-in brand protection, but allows additional terms

5. **MIT License** — maximally permissive
   - Minimal restrictions; no patent grant
   - Insufficient brand protection without additional terms

## Decision

Option 4: Apache License 2.0 with additional brand protection terms.

The standard Apache 2.0 text is used as the base license, supplemented with an addendum that explicitly reserves the "Fibonsai" and "Cryptomeria" marks and prohibits their use for endorsing derivative works without prior written permission.

This approach:
- Provides a well-known, permissive license that maximises adoption and contribution
- The patent grant protects contributors and users
- Brand protection terms sit as a separate, clearly marked addendum without modifying the Apache license text itself

## Consequences

### Positive
- Clear legal framework for external contributors
- Apache 2.0 is widely understood and accepted in both Python and Rust ecosystems
- SPDX `Apache-2.0` identifier works for both `pyproject.toml` and `Cargo.toml`
- Brand protection prevents confusion about official Fibonsai/Cryptomeria projects
- Patent grant reduces legal risk for all parties

### Negative
- Additional brand protection terms go beyond standard Apache 2.0, requiring users to review both sections
- Brand protection enforcement relies on Fibonsai actively policing trademark use
- Some permissive-license purists may prefer unmodified Apache 2.0

## Status

Accepted

## Related

- Issue #124
- PR #125
- CONTRIBUTIONS.md, CODE_OF_CONDUCT.md, SECURITY.md — companion governance files added alongside this license
