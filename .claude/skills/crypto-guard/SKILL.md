---
name: crypto-guard
version: 1.0.0
description: |
  Guardrail for the frozen derivation scheme. Use whenever editing core/ or anything
  that could touch key/password derivation - the KDF, domain-separation strings,
  constants, byte order, or the paper round-trip. Proactively apply when a change
  lands in core/; also use when asked to "change the crypto", "touch derivation",
  or "add an input to the password".
allowed-tools:
  - Bash
  - Read
---

# crypto-guard

Svitok's whole promise: a seed and phrase written on paper today produce the same
passwords forever. The derivation scheme is **frozen**. See CONTRIBUTING.md
("don't break the paper") and SPEC.md.

## Hard rules

- Never change derivation constants, domain-separation strings, byte order, or the
  KDF/derivation math in a way that alters output. If output changes, every seed
  already on paper is worthless.
- KDF *parameters* (`M`, `T`) may grow (they're written next to the seed). The
  *default* may change; the *algorithm* may not.
- Metadata (entry id, alias domains, display label) must **never** feed into
  derivation. The password is a function of site, login, counter, policy - nothing
  else. If a feature needs more inputs to `F`, it's the wrong design.
- A suspected flaw in the scheme is an issue to discuss, not a quiet change.

## Every time you touch this area

Run the vectors before and after - they pin the exact bytes:

```bash
cargo test --workspace          # includes core/tests/golden.rs
```

If `golden.rs` fails, you've broken bit-compatibility. Stop, revert, and reconsider -
this is never something to "update the expected values" for unless you are
deliberately and explicitly minting a new scheme version with the maintainer's signoff.

In the diff, call out explicitly that derivation was touched and show the golden
vectors still pass, so the reviewer sees it up front.
