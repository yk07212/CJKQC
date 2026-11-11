# Manual language guide

This directory contains the CJKQC user manual. These rules apply whenever an
agent edits the `.tex` sources or regenerates `manual.pdf`.

## Audience

The readers are computational chemists and students, not software developers.
Write for someone who knows quantum chemistry and the command line but not
programming vocabulary.

## Required style

- Prefer plain language over programming language.
- Never write "row-major". For coordinate lists, say: "three numbers per atom,
  in atom order: x, y, z for the first atom, then x, y, z for the second, and
  so on."
- Use "entry" rather than "field" or "key".
- Use "list" rather than "array".
- Use "fixed order" rather than "canonical order".
- Use "full path" rather than "canonical path".
- Use "stored values" rather than "observables".
- Use "filtering steps" rather than "pipeline".
- Use "how CJKQC reads a file" rather than "parser" where possible.
- Use "source details" rather than "provenance".
- Use "whole number" rather than "integer".
- Use "line break" rather than "newline".
- Avoid "schema", "serialization", "object", "spec", "boolean",
  "deterministic", "thread", "worker", "byte-identical", "atomic replacement",
  "null", and "NaN" unless there is no plain alternative.
- If a technical term is unavoidable, explain it immediately in plain words.
- Keep useful command-line terms such as "flag", "option", "command line",
  "exit code", and "standard output", but explain them the first time they
  appear.
- Always give units explicitly and use the standard units of the manual.
- Use short, runnable examples with realistic values.

## Accuracy

- `rqm/` is the authoritative description of behavior. Do not invent features
  or wording that contradicts it.
- Keep the front-page update date (`\ManualDate` in `manual.tex`) current when
  the manual content changes.
- After edits, rebuild and check the PDF:

```sh
make -C documentation
```

- Verify that every long flag in `CJKQC --help` still appears in the PDF.
