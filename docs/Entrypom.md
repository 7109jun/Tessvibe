# Entrypom — Entry 헤드리스 시뮬레이터 (검증용)

`tess-ent` 가 만든 `.ent`(또는 `project.json`, `.tess`)를 **Python 표준 라이브러리만으로** 실행해 보고, 결과를 JSON/CSV 로 돌려주는 검증 도구입니다. 엔트리 사이트를 열지 않고 "이 작품이 의도대로 움직이는가"를 확인하는 데 씁니다.

```
python -m entrypom run   작품.tess|.ent|.json [옵션]
python -m entrypom check 작품                  # 실행 없이: 지원하지 않는 블록 검사
python -m entrypom inspect 작품                # 장면/오브젝트/변수/리스트/테이블 요약
```

`.tess` 를 주면 `tess-ent` 로 먼저 컴파일합니다(`PATH`, 저장소의 `bin/`, 또는 환경변수 `TESS_ENT`).
Python 3.8+ 필요, 외부 패키지 없음.

## 권장 검증 흐름 (AI 용)

1. `tess-ent check` → 컴파일 에러 0.
2. `python -m entrypom run 작품.tess --seconds 10` → `ok` 가 `true` 인지 확인.
3. 확인하고 싶은 값을 덤프:
   `python -m entrypom run 작품.tess --dump-on "var=점수" --vars 점수 --lists 기록`
4. 입력이 필요한 게임은 시나리오로 키·클릭·대답을 줍니다 (아래).

## 오류는 항상 JSON

| 상황 | 종료 코드 | 출력 |
| --- | --- | --- |
| 정상 실행 | 0 | 결과 JSON (`"ok": true`) |
| 실행 중 오류 | 1 | 결과 JSON 의 `"ok": false` + `error` (csv 형식이면 오류 JSON 은 stderr) |
| 파일/컴파일/인자 오류 | 2 | `{"ok": false, "error": {...}}` |

```json
{"ok": false, "error": {"code": "INDEX_OUT_OF_RANGE", "message": "리스트 번호 10 가 범위(1~4)를 벗어났습니다.",
  "object": "고양이", "script": "when_run_button_click", "block": "change_value_list_index",
  "block_id": "t4k5", "time": 1.87, "frame": 112}}
```

주요 `code`: `COMPILE_ERROR`(`errors` 에 Tess 진단), `FILE_NOT_FOUND`, `LOAD_ERROR`, `UNSUPPORTED_BLOCK`,
`INDEX_OUT_OF_RANGE`(리스트·문자열·테이블 범위), `UNKNOWN_VARIABLE/LIST/TABLE/FIELD/OBJECT/COSTUME/SCENE`,
`NOT_TEXT_BOX`, `NO_ANSWER`(ask 인데 대답이 없음), `STACK_OVERFLOW`, `STEP_LIMIT`, `CLONE_LIMIT`, `BAD_SCENARIO`, `BAD_ARGUMENT`, `INTERNAL_ERROR`.
파이썬 트레이스백은 절대 출력하지 않습니다.

## 결과 JSON

```json
{"ok": true, "reason": "finished", "time": 1.87, "frames": 112, "fps": 60,
 "final": {"variables": {...}, "lists": {...}, "locals": {...}, "tables": {...}},
 "snapshots": [{"trigger": "say", "time": 0.87, "frame": 52, "variables": {...}, "lists": {...}}],
 "events": [{"t": 0.87, "event": "say", "name": "고양이", "text": "..."}],
 "warnings": []}
```

`reason`: `finished`(실행 중인 코드가 없음) · `stop`(`stop all`) · `timeout`(`--seconds`/`--frames` 도달; `forever` 게임은 보통 이것) · `error`.

## 덤프: AI 가 특정 행동을 했을 때 값 꺼내기

**1) Tess 코드 안에서** — 신호 이름이 `dump` 또는 `dump:이름` 인 `send` 가 실행되는 순간 스냅샷이 찍힙니다. 옵션이 필요 없습니다.

```tess
send "dump:전투 끝"
```

**2) 명령줄에서** — `--dump-on` (여러 번 가능):

| 트리거 | 언제 |
| --- | --- |
| `end` / `error` / `stop` | 종료 / 오류 / `stop all` 시 |
| `every=초` · `at=초` · `frame=N` | 주기 · 특정 시각 · 특정 프레임 |
| `signal[=이름]` | 신호를 보낼 때 (이름 생략 = 모든 신호) |
| `var[=이름]` · `list[=이름]` | 변수·리스트 값이 바뀔 때 |
| `say` · `ask` | 말하기 · 묻기 블록 실행 시 |
| `scene` · `clone` · `key[=키]` · `click` | 장면 전환 · 복제 · 키 입력 · 클릭 |

범위: `--vars 점수,체력` `--lists 기록` `--no-locals` `--no-tables` `--objects`(위치·보임·말풍선 포함).
형식: `--format json`(기본) / `--format csv`, 파일 저장 `-o 결과.csv`, 최종 상태만 `--final-only`.

CSV 열: `snapshot,trigger,time,frame,scope,kind,name,index,value` — `scope` 는 `global`/오브젝트 이름/`table`, `kind` 는 `variable`/`list`/`cell`/`object`, 리스트·테이블 줄 번호는 `index`(1부터).

## 입력 시나리오

명령줄: `--press space@0.5`(키) · `--click 10,20@1`(클릭) · `--answer 철수`(ask 의 대답, 순서대로).
파일: `--scenario s.json`

```json
{"answers": ["철수"], "mouse": {"x": 0, "y": 0},
 "events": [
   {"t": 0.5, "do": "key_down", "key": "space"}, {"t": 0.6, "do": "key_up", "key": "space"},
   {"t": 1.0, "do": "click", "x": 10, "y": 20},
   {"t": 1.2, "do": "mouse_move", "x": 50, "y": 0},
   {"t": 2.0, "do": "signal", "name": "게임 시작"}, {"t": 3.0, "do": "answer", "text": "영희"}]}
```

그 밖의 옵션: `--seconds 60`(최대 가상 시간) `--frames N` `--fps N` `--seed 0`(난수; 같은 시드면 같은 결과) `--now 2026-01-01T09:00:00`(`now()` 고정) `--pretty`.

## Python API

```python
from entrypom import load_project, Simulator, Scenario
sim = Simulator(load_project("game.tess"), Scenario({"answers": ["철수"]}), dump_on=["signal=dump"])
result = sim.run()          # 위 JSON 과 같은 dict
```

## 시뮬레이션 규칙 (알아 둘 점)

- **가상 시간**: 프레임마다 `1/fps` 초가 흐르는 결정적 실행입니다. 실제로 기다리지 않아 빠릅니다.
- 반복문은 반복마다 한 프레임을 양보하고, 모든 `when` 스크립트는 병렬로 돌아갑니다. 같은 신호를 다시 받으면 새 스레드가 추가로 시작됩니다.
- 값 규칙은 엔트리와 같습니다: 숫자 문자열은 숫자처럼 비교, `+` 는 문자열이면 이어 붙임, `true/false` 저장은 `"TRUE"/"FALSE"`. 0 으로 나누면 `Infinity`/`NaN` 이 되고 변수에 저장되면 `warnings` 에 기록됩니다.
- 범위를 벗어난 리스트·문자열·테이블 접근은 오류로 멈춥니다(검증 목적).
- 충돌(`touching`)은 회전된 사각형(SAT) 기준이며 보이는 오브젝트끼리만 판정합니다. 벽은 ±240 × ±135.
- 테이블 셀 주소 `B2` 는 1줄 = 머리글(열 이름), 2줄부터 데이터입니다.
- 소리·그리기·TTS·화면 표시 블록은 아무 일도 하지 않고(소리 길이만큼은 기다림), 날씨·번역 같은 네트워크 블록은 빈 값을 돌려주며 `warnings` 에 `STUBBED_BLOCK` 으로 남습니다.
- 값 블록 함수 안의 `wait`/반복은 그 자리에서 즉시 끝까지 실행됩니다.
- `skip`(화면 갱신 생략)은 양보 없이 지나갑니다.

테스트: `python -m unittest tests.test_entrypom`
