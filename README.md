ddtt786님이 제작하신 Tess를 바탕으로 만들었으며. 코드를 찾아보니 MIT 관련을 봐서 MIT로 정하였습니다. 법적 문제가 발생할시 즉시 라이선스를 바꾸거나. 저장소를 삭제하겠습니다.

# tess-ent

Rust converter: Tess source (github.com/ddtt786/tess @ 97d2266) -> Entry `.ent`.

    tess-ent check main.tess --json      # diagnostics for AI self-repair
    tess-ent build main.tess -o out.ent  # --assets DIR, --name N, --strict
    tess-ent ast main.tess               # AST as JSON
    tess-ent ref [language|compile|workflow]
    tess-ent mcp                         # MCP stdio server: tess_reference / tess_check / tess_build

Output is verified identical to the TypeScript reference (project JSON, IDs, diagnostics, asset paths)
on 240 sample programs. Not ported: `run`, `decompile`. Thumbnails are PNG-equivalent in size but not byte-identical.

## License

MIT (see `LICENSE`). Third-party attribution: `THIRD_PARTY_NOTICES.md`.
