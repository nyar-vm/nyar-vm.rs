# interpreter tests

Runtime facade tests only.

## Responsibilities

- Cover `RuntimeFamily`, run templates, and command expansion—pure runtime semantics.
- Ensure `nyar-runner` consumes compiled artifacts only; no guest-language source awareness.
- Real source fixtures and integration smoke live in `legion/tests`.
