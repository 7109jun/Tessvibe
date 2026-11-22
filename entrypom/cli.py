"""Entrypom CLI:  python -m entrypom run|check|inspect FILE"""
import argparse
import csv
import datetime
import io
import json
import math
import sys

from .project import EntrypomError, load_project
from .sim import Scenario, Simulator, static_check
from .values import out_value

USAGE_EXIT, FAIL_EXIT = 2, 1


def clean(v):
    if isinstance(v, float) and (v != v or v in (math.inf, -math.inf)):
        return out_value(v)
    if isinstance(v, dict):
        return {k: clean(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [clean(x) for x in v]
    return v


def dumps(v, pretty):
    return json.dumps(clean(v), ensure_ascii=False, indent=2 if pretty else None)


def csv_rows(snapshots):
    yield ["snapshot", "trigger", "time", "frame", "scope", "kind", "name", "index", "value"]
    for i, s in enumerate(snapshots):
        base = [i, s["trigger"], s["time"], s["frame"]]
        for k, v in s.get("variables", {}).items():
            yield base + ["global", "variable", k, "", out_value(v)]
        for k, lst in s.get("lists", {}).items():
            for j, v in enumerate(lst, 1):
                yield base + ["global", "list", k, j, out_value(v)]
        for obj, d in s.get("locals", {}).items():
            for dd in (d if isinstance(d, list) else [d]):
                for k, v in dd["variables"].items():
                    yield base + [obj, "variable", k, "", out_value(v)]
                for k, lst in dd["lists"].items():
                    for j, v in enumerate(lst, 1):
                        yield base + [obj, "list", k, j, out_value(v)]
        for tname, t in s.get("tables", {}).items():
            for r, row in enumerate(t["rows"], 1):
                for f, v in zip(t["fields"], row):
                    yield base + ["table", "cell", "%s.%s" % (tname, f), r, v]
        for ob in s.get("objects", []):
            for k, v in ob.items():
                if k != "name":
                    yield base + [ob["name"], "object", k, "", out_value(v)]


def to_csv(snapshots):
    buf = io.StringIO()
    w = csv.writer(buf, lineterminator="\n")
    for row in csv_rows(snapshots):
        w.writerow(row)
    return buf.getvalue()


def split(v):
    return [x.strip() for x in v.split(",") if x.strip()] if v else None


def parse_at(spec, what):
    body, _, t = spec.partition("@")
    try:
        return body, float(t) if t else 0.0
    except ValueError:
        raise EntrypomError("BAD_ARGUMENT", "--%s 형식: 값@초 (예: space@0.5), 받은 값: %s" % (what, spec))


def build_parser():
    ap = argparse.ArgumentParser(prog="entrypom", description="Entry(엔트리) 작품을 헤드리스로 실행하는 시뮬레이터")
    sub = ap.add_subparsers(dest="cmd")
    run = sub.add_parser("run", help="작품 실행")
    run.add_argument("file", help=".ent / project.json / .tess")
    run.add_argument("--scenario", help="입력 시나리오 JSON 파일")
    run.add_argument("--press", action="append", default=[], metavar="키@초", help="키 누름 (예: space@0.5)")
    run.add_argument("--click", action="append", default=[], metavar="x,y@초", help="마우스 클릭 (예: 10,20@1)")
    run.add_argument("--answer", action="append", default=[], help="ask 블록에 줄 대답 (여러 번)")
    run.add_argument("--seconds", type=float, default=60.0, help="최대 가상 시간(초), 기본 60")
    run.add_argument("--frames", type=int, help="최대 프레임 수 (--seconds 대신)")
    run.add_argument("--fps", type=float, help="초당 프레임 (기본: 작품 설정)")
    run.add_argument("--seed", type=int, default=0, help="난수 시드 (기본 0)")
    run.add_argument("--now", help="get_date 가 돌려줄 고정 시각 (ISO, 예: 2026-01-01T09:00:00)")
    run.add_argument("--dump-on", action="append", default=[], metavar="트리거",
                     help="이때 변수/리스트를 덤프: end error stop every=초 at=초 frame=N signal[=이름] var[=이름] list[=이름] say ask scene clone key[=이름] click")
    run.add_argument("--vars", help="덤프할 변수 이름 (쉼표 구분, 기본 전체)")
    run.add_argument("--lists", help="덤프할 리스트 이름 (쉼표 구분, 기본 전체)")
    run.add_argument("--no-locals", action="store_true", help="오브젝트 지역 변수 제외")
    run.add_argument("--no-tables", action="store_true", help="테이블 제외")
    run.add_argument("--objects", action="store_true", help="오브젝트 상태(x,y,보임 등) 포함")
    run.add_argument("--format", choices=["json", "csv"], default="json")
    run.add_argument("--final-only", action="store_true", help="스냅샷 없이 최종 상태만 (json)")
    run.add_argument("-o", "--out", help="결과를 파일로 저장 (기본: 표준출력)")
    run.add_argument("--pretty", action="store_true", help="JSON 들여쓰기")
    chk = sub.add_parser("check", help="실행 없이 불러오기와 지원 여부만 검사 (JSON 출력)")
    chk.add_argument("file")
    chk.add_argument("--pretty", action="store_true")
    ins = sub.add_parser("inspect", help="작품 구조 요약 (JSON 출력)")
    ins.add_argument("file")
    ins.add_argument("--pretty", action="store_true")
    return ap


def emit_error(err, pretty, to_stderr=False):
    doc = {"ok": False, "error": err.to_dict() if isinstance(err, EntrypomError) else {"code": "ERROR", "message": str(err)}}
    print(dumps(doc, pretty), file=sys.stderr if to_stderr else sys.stdout)


def make_scenario(a):
    data = {}
    if a.scenario:
        try:
            with open(a.scenario, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError) as e:
            raise EntrypomError("BAD_SCENARIO", "시나리오를 읽지 못했습니다: %s" % e, file=a.scenario)
    sc = Scenario(data)
    sc.answers += a.answer
    for s in a.press:
        k, t = parse_at(s, "press")
        sc.press(k, t)
    for s in a.click:
        body, t = parse_at(s, "click")
        try:
            x, y = [float(v) for v in body.split(",")]
        except ValueError:
            raise EntrypomError("BAD_ARGUMENT", "--click 형식: x,y@초, 받은 값: %s" % s)
        sc.click(x, y, t)
    return sc


def cmd_run(a):
    project = load_project(a.file)
    now = None
    if a.now:
        try:
            now = datetime.datetime.fromisoformat(a.now)
        except ValueError:
            raise EntrypomError("BAD_ARGUMENT", "--now 는 ISO 형식이어야 합니다: %s" % a.now)
    sim = Simulator(project, make_scenario(a), fps=a.fps, seed=a.seed, now=now, dump_on=a.dump_on,
                    max_seconds=a.seconds, max_frames=a.frames,
                    snapshot_opts={"vars": split(a.vars), "lists": split(a.lists), "locals": not a.no_locals,
                                   "tables": not a.no_tables, "objects": a.objects})
    res = sim.run()
    comp = project.get("__compile")
    if comp and comp.get("warnings"):
        res["compile_warnings"] = comp["warnings"]
    if a.final_only:
        res["snapshots"] = []
    if a.format == "csv":
        snaps = list(res["snapshots"])
        if not snaps or a.final_only or True:
            f = dict(res["final"])
            f["trigger"] = "final"
            snaps.append(f)
        text = to_csv(snaps)
        if not res["ok"]:
            emit_error(EntrypomError(res["error"]["code"], res["error"]["message"],
                                     **{k: v for k, v in res["error"].items() if k not in ("code", "message")}), a.pretty, True)
    else:
        text = dumps(res, a.pretty) + "\n"
    if a.out:
        with open(a.out, "w", encoding="utf-8", newline="") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0 if res["ok"] else FAIL_EXIT


def cmd_check(a):
    project = load_project(a.file)
    bad = static_check(project)
    sim = Simulator(project, max_seconds=0)
    doc = {"ok": not bad, "unsupported_blocks": bad, "summary": summary(project, sim),
           "compile_warnings": (project.get("__compile") or {}).get("warnings", [])}
    print(dumps(doc, a.pretty))
    return 0 if not bad else FAIL_EXIT


def summary(project, sim=None):
    return {
        "scenes": [s.get("name") for s in project.get("scenes", [])],
        "objects": [{"name": o.get("name"), "type": o.get("objectType")} for o in project.get("objects", [])],
        "variables": [v["name"] for v in project.get("variables", []) if v.get("variableType") in ("variable", None) and not v.get("object")],
        "lists": [v["name"] for v in project.get("variables", []) if v.get("variableType") == "list"],
        "tables": [t.get("name") for t in project.get("tables", [])],
        "signals": [m.get("name") for m in project.get("messages", [])],
        "functions": len(project.get("functions", [])),
        "blocks": getattr(sim, "block_total", None),
        "fps": project.get("speed"),
    }


def cmd_inspect(a):
    project = load_project(a.file)
    print(dumps({"ok": True, "name": project.get("name"), **summary(project, Simulator(project, max_seconds=0))}, a.pretty))
    return 0


def main(argv=None):
    ap = build_parser()
    try:
        a = ap.parse_args(argv)
    except SystemExit as e:
        return USAGE_EXIT if e.code else 0
    if not a.cmd:
        ap.print_help()
        return USAGE_EXIT
    pretty = getattr(a, "pretty", False)
    try:
        return {"run": cmd_run, "check": cmd_check, "inspect": cmd_inspect}[a.cmd](a)
    except EntrypomError as e:
        emit_error(e, pretty)
        return USAGE_EXIT
    except KeyboardInterrupt:
        return 130
    except Exception as e:  # never leak a traceback: always JSON
        emit_error(EntrypomError("INTERNAL_ERROR", "%s: %s" % (type(e).__name__, e)), pretty)
        return USAGE_EXIT
