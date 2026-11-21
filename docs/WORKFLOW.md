# Writing Tess for tess-ent (AI workflow)

1. Read the language reference: `tess-ent ref` (and `tess-ent ref compile` for compile-time behaviour).
2. Write the program in a `.tess` file. Costume/sound files are referenced by relative path and must exist next to the source (or in an `--assets` folder).
3. Validate: `tess-ent check main.tess --json` and fix every `errors` entry (`line`/`column`/`message`). Entries in `notices` mean "resolved at run time", not failures.
4. Build: `tess-ent build main.tess -o out.ent` (add `--strict` to refuse a build that has any error).
5. Upload the `.ent` at playentry.org (My Work → Import).

Rules that cause most errors:
- Blocks open with `:`, `do` or `then` and close with `end`.
- Object names, scene names, signal names and sound/costume lookups are string literals.
- Variable/list initial values must be constants; compute inside `when start`.
- A function returns a value only through a single `return` as its last statement.
