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

## 실행 파일

`bin/tess-ent-linux-x86_64` (Linux x86_64).

`bin/` 의 Windows(`tess-ent-windows-x86_64.exe`)·Linux·macOS 실행 파일은 GitHub Actions(`.github/workflows/build.yml`)가 `main` 푸시마다 빌드해 자동으로 커밋합니다. `v*` 태그(예: `v0.1.0`)를 푸시하면 Releases 에 Windows/Linux/macOS 실행 파일이 올라갑니다. 직접 빌드하려면 `cargo build --release` 후 `target/release/tess-ent`  쓰세요.

## CLI 사용법

```bash
chmod +x bin/tess-ent-linux-x86_64
./bin/tess-ent-linux-x86_64 <명령> [파일] [옵션]
```

| 명령 | 설명 |
| --- | --- |
| `check <파일.tess>` | 문법·의미 검사. 에러/경고/주의를 줄·칸과 함께 출력 |
| `build <파일.tess>` | 엔트리 작품(`.ent`)으로 컴파일. 기본 출력은 `<파일>.ent` |
| `ast <파일.tess>` | AST 를 JSON 으로 출력 |
| `ref [language\|compile\|workflow]` | AI 용 Tess 언어 레퍼런스 출력 |
| `mcp` | MCP(stdio) 서버로 실행 (`tess_reference`, `tess_check`, `tess_build`) |

| 옵션 | 설명 |
| --- | --- |
| `-o`, `--out <경로>` | 출력 경로. `.json` 이면 `project.json`, 그 외에는 `.ent` |
| `--assets <폴더>` | 모양·소리 파일을 찾을 폴더 (여러 번 지정 가능, 기본: 소스 파일이 있는 폴더) |
| `--name <이름>` | 작품 이름 (기본: `project` 블록의 `title`) |
| `--strict` | 에러가 하나라도 있으면 중단, 주의를 경고로 올림 |
| `--json` | 진단 결과를 JSON 으로 출력 (`check`/`build`) |

종료 코드는 성공 `0`, 실패 `1` 입니다.

### 예시

```bash
./bin/tess-ent-linux-x86_64 check main.tess --json
./bin/tess-ent-linux-x86_64 build main.tess -o out/game.ent --assets ./assets --strict
./bin/tess-ent-linux-x86_64 ref > tess-reference.md
```

### AI 연동 (MCP)

```json
{ "mcpServers": { "tess": { "command": "/절대경로/bin/tess-ent-linux-x86_64", "args": ["mcp"] } } }
```

AI 작업 순서: `ref` 로 문법 읽기 → `.tess` 작성 → `check --json` 으로 에러 수정 → `build` → playentry.org 에서 `.ent` 불러옵니다

## License

MIT (see `LICENSE`). Third-party attribution: `THIRD_PARTY_NOTICES.md`.


## Entrypom (검증용 시뮬레이터)

외부 라이브러리 없는 Python 으로 `.ent`/`.tess` 를 헤드리스 실행해 오류와 변수·리스트를 JSON/CSV 로 돌려줍니다.

```
python -m entrypom run 게임.tess --answer 철수 --press space@1 --dump-on "var=점수" --vars 점수 --format csv
python -m entrypom check 게임.ent
```

소스(`entrypom/`)와 실행 파일(`bin/entrypom.pyz`, 단일 파일·Python 3.8+ 만 있으면 실행)이 둘 다 들어 있습니다.

```
python bin/entrypom.pyz run 게임.ent --seconds 10 --pretty      # 실행 (JSON 출력)
python bin/entrypom.pyz run 게임.tess --dump-on "var=점수" --vars 점수 --format csv -o 결과.csv
python bin/entrypom.pyz check 게임.ent                          # 지원하지 않는 블록 검사
python bin/entrypom.pyz inspect 게임.ent                        # 구조 요약
```

| 옵션 | 뜻 |
| --- | --- |
| `--answer 텍스트` / `--press 키@초` / `--click x,y@초` / `--scenario 파일.json` | 입력 주기 (ask 대답, 키, 클릭) |
| `--seconds N` / `--frames N` / `--fps N` / `--seed N` / `--now ISO` | 실행 길이·결정성 |
| `--dump-on 트리거` | `var=이름` `list` `signal` `say` `ask` `every=초` `at=초` `frame=N` `end` `error` … (여러 번) |
| `--vars a,b` / `--lists c` / `--no-locals` / `--no-tables` / `--objects` | 덤프 범위 |
| `--format json\|csv` / `-o 파일` / `--final-only` / `--pretty` | 출력 형식 |

오류는 항상 JSON(`{"ok": false, "error": {...}}`), 종료 코드는 0 정상 · 1 실행 오류 · 2 파일/컴파일/인자 오류입니다. Tess 코드에서 `send "dump"` 를 보내면 그 순간의 변수·리스트가 자동으로 덤프됩니다.

자세한 사용법: [docs/Entrypom.md](docs/Entrypom.md) · AI 도구(Claude Code/Codex)·사람용 가이드: [docs/Usage-Guide.md](docs/Usage-Guide.md)
