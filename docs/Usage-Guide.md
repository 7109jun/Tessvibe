# 사용 가이드 — AI 코딩 도구 + Entrypom 검증 + 사람이 직접 쓰기

전체 흐름은 한 줄입니다.

```
AI 가 Tess 작성 → tess-ent check/build (.ent) → Entrypom 으로 실행·검증 → 엔트리에서 열기
```

## 1. 준비

저장소를 받고 `bin/` 의 파일을 씁니다. (Rust/Python 외 설치 없음, Entrypom 은 Python 3.8+ 만 필요)

| 파일 | 용도 |
| --- | --- |
| `bin/tess-ent-windows-x86_64.exe` · `tess-ent-linux-x86_64` · `tess-ent-macos-arm64` | Tess → `.ent` 변환기 |
| `bin/entrypom.pyz` | 시뮬레이터 (`python bin/entrypom.pyz ...`) |

macOS/Linux 에서는 `chmod +x bin/tess-ent-*` 가 필요할 수 있습니다. 편의를 위해 PATH 에 `tess-ent` 이름으로 두면 Entrypom 이 `.tess` 를 바로 컴파일할 수 있습니다(`TESS_ENT` 환경변수로 경로 지정도 가능).

## 2. Claude Code · Codex 같은 AI 코딩 도구와 함께 쓰기

AI 에게는 **언어 설명서**와 **검증 명령**만 알려 주면 됩니다.

### 2-1. 설명서 주기

- 프로젝트 루트의 `AGENTS.md`(Codex 등) 또는 `CLAUDE.md`(Claude Code)에 아래를 적어 둡니다.

```markdown
## Tess → 엔트리 작품
- 엔트리 작품은 Tess 언어(.tess)로 작성한다. 문법은 `docs/The-Tess-Programming-Language.md` 를 먼저 읽는다.
- 작성 후 반드시 순서대로 실행한다:
  1. `bin/tess-ent-linux-x86_64 check 작품.tess --json`   (errors 가 0 이 될 때까지 수정)
  2. `python bin/entrypom.pyz run 작품.tess --seconds 10 --pretty`   (ok:true 확인)
  3. `bin/tess-ent-linux-x86_64 build 작품.tess -o 작품.ent`
- 값 확인이 필요하면 코드에 `send "dump"` 를 넣거나 `--dump-on` 을 쓴다. 자세한 옵션은 `docs/Entrypom.md`.
- 실행 파일 이름은 OS 에 맞게 바꾼다(Windows: `bin\tess-ent-windows-x86_64.exe`).
```

- 언어 설명서는 `tess-ent ref` 로도 출력됩니다(`tess-ent ref compile`, `tess-ent ref workflow`).

### 2-2. MCP 로 연결 (선택)

MCP 를 지원하는 도구(Claude Code, Codex 등)에는 변환기를 MCP 서버로 등록합니다.

```json
{ "mcpServers": { "tess": { "command": "/절대경로/bin/tess-ent-linux-x86_64", "args": ["mcp"] } } }
```

AI 는 `tess_reference` → `tess_check` → `tess_build` 순서로 부릅니다. Entrypom 검증은 MCP 가 아니라 셸 명령(`python bin/entrypom.pyz ...`)이므로, 도구가 셸을 실행할 수 있게 허용해 두세요.

### 2-3. 요청 예시

> "docs/The-Tess-Programming-Language.md 를 읽고, 공을 피하는 게임을 Tess 로 만들어 줘. check 와 Entrypom 실행이 통과하면 .ent 로 빌드해. 점수는 `send "dump:게임 끝"` 로 확인해 줘."

## 3. Entrypom 으로 검증하기

### 3-1. 기본

```
python bin/entrypom.pyz run 작품.tess --seconds 10 --pretty
```

- `"ok": true` 이면 실행 중 오류가 없다는 뜻입니다. `reason` 이 `timeout` 이면 `forever` 로 계속 도는 작품이라 정상입니다.
- 오류가 나면 JSON 으로 위치가 나옵니다(`error.code`, `object`, `block`, `time`). AI 에게 그 JSON 을 그대로 주고 고치게 하면 됩니다.
- `check` 는 지원하지 않는 블록만 빠르게 확인합니다: `python bin/entrypom.pyz check 작품.ent`.

### 3-2. 값 확인 (덤프)

```
# 점수가 바뀔 때마다 기록해서 CSV 로
python bin/entrypom.pyz run 작품.tess --dump-on "var=점수" --vars 점수 --format csv -o 결과.csv

# 2초 시점의 변수/리스트
python bin/entrypom.pyz run 작품.tess --dump-on at=2 --pretty
```

코드 안에서 원하는 시점에 찍으려면 신호를 보냅니다: `send "dump"` 또는 `send "dump:이름"`.

### 3-3. 입력이 필요한 작품

```
python bin/entrypom.pyz run 작품.tess --press space@0.5 --click 10,20@1 --answer 철수 --seconds 5
```

복잡한 입력은 `--scenario 시나리오.json` (형식은 `docs/Entrypom.md`).

### 3-4. 결과가 이상할 때

- 같은 `--seed` 는 같은 결과를 냅니다. 난수가 있는 작품은 시드를 바꿔 여러 번 돌려 보세요.
- `warnings` 의 `STUBBED_BLOCK` 은 날씨·번역 같은 네트워크 블록이 빈 값으로 처리됐다는 뜻입니다.
- Entrypom 은 실제 엔트리와 100% 같지 않습니다(충돌은 사각형 근사, 소리·그리기는 동작 안 함). 최종 확인은 엔트리에서 `.ent` 를 열어 한 번 해 보세요.

## 4. 사람이 직접 쓰기 (AI 없이)

1. 텍스트 파일 `game.tess` 를 만들고 [docs/The-Tess-Programming-Language.md](The-Tess-Programming-Language.md) 를 보며 작성합니다. 모양·소리 파일은 `.tess` 와 같은 폴더에 둡니다.
2. 문법 검사: `tess-ent check game.tess`
3. 실행해 보기: `python bin/entrypom.pyz run game.tess --seconds 10 --pretty`
4. 내보내기: `tess-ent build game.tess -o game.ent`
5. 엔트리(playentry.org)의 작품 불러오기로 `game.ent` 를 엽니다.

자주 쓰는 옵션: `tess-ent build 파일 --assets 폴더`(리소스 폴더), `--name 이름`, `--strict`(에러가 있으면 중단), `--json`(진단을 JSON 으로).

## 5. 문제 해결

| 증상 | 해결 |
| --- | --- |
| `TESS_ENT_NOT_FOUND` | `tess-ent` 를 PATH 에 두거나 `TESS_ENT=경로` 지정, 또는 `.tess` 대신 `.ent` 로 실행 |
| `COMPILE_ERROR` | `errors` 의 줄·열을 보고 Tess 문법 수정 (`tess-ent check` 와 동일) |
| `NO_ANSWER` | 작품이 `ask` 를 쓰므로 `--answer` 로 대답 지정 |
| `INDEX_OUT_OF_RANGE` | 리스트/문자열/테이블 번호(1부터) 확인 |
| `STEP_LIMIT` · `STACK_OVERFLOW` | 무한 재귀 또는 비정상 루프 확인 |
| 실행 파일 권한 거부 | `chmod +x bin/파일` |

더 자세히: [Entrypom.md](Entrypom.md) · [WORKFLOW.md](WORKFLOW.md) · [tess-compile.md](tess-compile.md)
