# Local patch of parakeet-rs 0.4.4 (crates.io)

dictate needs two hooks in greedy TDT decoding that parakeet-rs does not have, so this copy adds
them:

1. **Allowed tokens** — keeps Parakeet V3/Ultra from switching to Cyrillic on short German
   utterances.
   - `src/model_tdt.rs`: `ParakeetTDTModel.allowed_tokens: Option<Vec<bool>>`; `pick()` takes it
     and never returns a disallowed token (blank is always allowed).
   - `src/parakeet_tdt.rs`: `ParakeetTDT::set_allowed_tokens(Option<Vec<bool>>)`.

2. **Token boost (context biasing)** — favours dictionary words while decoding (dictate issue #6).
   - `src/model_tdt.rs`: `pub struct TokenBoost { next: Vec<Vec<BoostEdge>>, word_start: Vec<bool> }`
     and `pub struct BoostEdge { token, to, bonus, new_word }`, a token automaton (node 0 = start,
     edges sorted by token). `ParakeetTDTModel.boost: Option<TokenBoost>`; `greedy_decode()` keeps
     the current node and steps it on every emitted token (`TokenBoost::step`: own edge, else the
     start node's edge, else back to the start). `pick()` takes `Option<(&TokenBoost, node)>`:
     when the model's choice is not blank, the node's edge tokens compete with `logit + bonus`
     (masked tokens excluded). A blank is never overridden, and a `new_word` edge only competes
     when the model's choice is itself a word start (`word_start`).
   - `src/parakeet_tdt.rs`: `ParakeetTDT::set_boost(Option<TokenBoost>)`.
   - `src/lib.rs`: exports `TokenBoost`, `BoostEdge`.
   - Building the automaton from words and `vocab.txt` lives in dictate (`core/src/boost.rs`, crate dictate-core).

Everything else is the crates.io release (examples/ and scripts/ removed). Worth proposing upstream;
drop this folder and the `[patch.crates-io]` entry in Cargo.toml once a release has it.
