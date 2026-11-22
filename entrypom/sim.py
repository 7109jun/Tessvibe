"""Entrypom core: scheduler, objects, events, snapshots."""
import copy
import json
import math
import random
import time as _time

from .project import EntrypomError
from .values import out_value, parse_number, to_num, to_str, truthy, norm
from .blocks import Blocks

KEY_NAMES = {
    "backspace": 8, "tab": 9, "enter": 13, "shift": 16, "ctrl": 17, "alt": 18, "esc": 27, "escape": 27,
    "space": 32, "left": 37, "up": 38, "right": 39, "down": 40, "capslock": 20, "pageup": 33,
    "pagedown": 34, "end": 35, "home": 36, "insert": 45, "delete": 46, ";": 186, "=": 187, ",": 188,
    "-": 189, ".": 190, "/": 191, "~": 192, "[": 219, "\\": 220, "backslash": 220, "]": 221, "'": 222,
}


def key_code(k):
    if isinstance(k, int):
        return k
    s = str(k).strip().lower()
    if s.isdigit() and len(s) > 1:
        return int(s)
    if s in KEY_NAMES:
        return KEY_NAMES[s]
    if len(s) == 1 and s.isdigit():
        return 48 + int(s)
    if len(s) == 1 and "a" <= s <= "z":
        return 65 + ord(s) - 97
    if s.startswith("f") and s[1:].isdigit() and 1 <= int(s[1:]) <= 12:
        return 111 + int(s[1:])
    if s.startswith("numpad") and s[6:].isdigit():
        return 96 + int(s[6:])
    raise EntrypomError("BAD_SCENARIO", "알 수 없는 키 이름: %r" % (k,))


class SimError(EntrypomError):
    pass


class Scenario:
    """Inputs fed to the project: keys, mouse clicks, answers, signals."""

    def __init__(self, data=None):
        data = data or {}
        self.answers = list(data.get("answers", []))
        self.mouse = dict(data.get("mouse", {"x": 0, "y": 0}))
        self.user = data.get("user", "guest")
        self.events = []
        for e in data.get("events", []):
            self.add_event(e)

    def add_event(self, e):
        if not isinstance(e, dict) or "do" not in e:
            raise EntrypomError("BAD_SCENARIO", "events 항목에는 'do' 가 필요합니다: %r" % (e,))
        ev = dict(e)
        ev["t"] = float(ev.get("t", 0))
        if ev["do"] in ("key_down", "key_up", "press"):
            ev["code"] = key_code(ev.get("key"))
        self.events.append(ev)
        self.events.sort(key=lambda x: x["t"])

    def press(self, key, t=0.0, hold=0.1):
        self.add_event({"t": t, "do": "key_down", "key": key})
        self.add_event({"t": t + hold, "do": "key_up", "key": key})

    def click(self, x, y, t=0.0, hold=0.1):
        self.add_event({"t": t, "do": "mouse_down", "x": x, "y": y})
        self.add_event({"t": t + hold, "do": "mouse_up", "x": x, "y": y})


class Obj:
    def __init__(self, data, scene_id, project_vars):
        self.id = data["id"]
        self.name = data.get("name", self.id)
        self.kind = data.get("objectType", "sprite")
        self.scene = data.get("scene", scene_id)
        e = data.get("entity", {})
        self.e = dict(e)
        self.x = to_num(e.get("x", 0))
        self.y = to_num(e.get("y", 0))
        self.sx = to_num(e.get("scaleX", 1), 1)
        self.sy = to_num(e.get("scaleY", 1), 1)
        self.init_sx, self.init_sy = self.sx, self.sy
        self.rotation = to_num(e.get("rotation", 0))
        self.direction = to_num(e.get("direction", 90), 90)
        self.w = to_num(e.get("width", 100), 100)
        self.h = to_num(e.get("height", 100), 100)
        self.visible = bool(e.get("visible", True))
        self.text = e.get("text") if self.kind == "textBox" else None
        sprite = data.get("sprite", {}) or {}
        self.pictures = list(sprite.get("pictures", []))
        self.sounds = list(sprite.get("sounds", []))
        sel = data.get("selectedPictureId")
        self.pic = 0
        for i, p in enumerate(self.pictures):
            if p.get("id") == sel:
                self.pic = i
        self.vars = {}
        self.lists = {}
        for v in project_vars:
            if v.get("object") == self.id:
                if v.get("variableType") == "list":
                    self.lists[v["id"]] = [i.get("data") for i in v.get("array", [])]
                else:
                    self.vars[v["id"]] = v.get("value", 0)
        self.effects = {}
        self.dialog = None
        self.is_clone = False
        self.parent = None
        self.alive = True
        self.drawing = False
        self.props = {}

    @property
    def size(self):
        return (self.w * abs(self.sx) + self.h * abs(self.sy)) / 2

    def corners(self):
        hw, hh = self.w * abs(self.sx) / 2, self.h * abs(self.sy) / 2
        r = math.radians(self.rotation)
        c, s = math.cos(r), math.sin(r)
        pts = []
        for px, py in ((-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)):
            pts.append((self.x + px * c + py * s, self.y - px * s + py * c))
        return pts

    def clone_of(self, sim_vars):
        o = copy.copy(self)
        o.e = dict(self.e)
        o.vars = copy.deepcopy(self.vars)
        o.lists = copy.deepcopy(self.lists)
        o.effects = dict(self.effects)
        o.props = dict(self.props)
        o.dialog = None
        o.is_clone = True
        o.parent = self.id
        o.alive = True
        return o


class Thread:
    __slots__ = ("gen", "obj", "kind", "dead", "frames", "waiters", "tag")

    def __init__(self, obj, kind, tag=None):
        self.gen = None
        self.obj = obj
        self.kind = kind
        self.dead = False
        self.frames = []
        self.waiters = []
        self.tag = tag


def poly_hit(a, b):
    for poly in (a, b):
        n = len(poly)
        for i in range(n):
            x1, y1 = poly[i]
            x2, y2 = poly[(i + 1) % n]
            nx, ny = y1 - y2, x2 - x1
            if nx == 0 and ny == 0:
                continue
            pa = [nx * px + ny * py for px, py in a]
            pb = [nx * px + ny * py for px, py in b]
            if max(pa) < min(pb) or max(pb) < min(pa):
                return False
    return True


def point_in(poly, x, y):
    return poly_hit(poly, [(x - 0.01, y - 0.01), (x + 0.01, y - 0.01), (x + 0.01, y + 0.01), (x - 0.01, y + 0.01)])


class Simulator(Blocks):
    STAGE_W, STAGE_H = 480, 270

    def __init__(self, project, scenario=None, fps=None, seed=0, now=None, dump_on=None,
                 max_seconds=60.0, max_frames=None, step_limit=2_000_000, snapshot_opts=None):
        self.project = project
        self.scenario = scenario or Scenario()
        self.fps = float(fps or project.get("speed") or 60)
        self.dt = 1.0 / self.fps
        self.rng = random.Random(seed)
        self.now = now
        self.max_seconds = max_seconds
        self.max_frames = max_frames
        self.step_limit = step_limit
        self.snap_opts = {"vars": None, "lists": None, "locals": True, "objects": False, "tables": True}
        self.snap_opts.update(snapshot_opts or {})
        self.dump_specs = [self._parse_spec(s) for s in (dump_on or [])]
        self.warnings = []
        self._warned = set()
        self._index()
        self.reset()

    # ---------- setup ----------
    def _warn(self, code, message, **kw):
        key = (code, message)
        if key in self._warned:
            return
        self._warned.add(key)
        w = {"code": code, "message": message}
        w.update(kw)
        self.warnings.append(w)

    def _index(self):
        p = self.project
        self.scene_ids = [s["id"] for s in p.get("scenes", [])] or [None]
        self.scene_names = {s["id"]: s.get("name") for s in p.get("scenes", [])}
        self.msg_names = {m["id"]: m.get("name") for m in p.get("messages", [])}
        self.msg_ids = {}
        for m in p.get("messages", []):
            self.msg_ids[m.get("name")] = m["id"]
        self.var_defs = [v for v in p.get("variables", [])]
        self.fdefs = {}
        for f in p.get("functions", []):
            self.fdefs[f["id"]] = self._parse_function(f)
        self.scripts = {}  # obj id -> list of (head block, statements)
        self.block_total = 0
        for o in p.get("objects", []):
            lst = []
            try:
                data = json.loads(o.get("script") or "[]")
            except ValueError:
                data = []
            for chain in data:
                if not chain:
                    continue
                lst.append((chain[0], chain[1:]))
                self.block_total += self._count(chain)
            self.scripts[o["id"]] = lst

    def _count(self, v):
        if isinstance(v, list):
            return sum(self._count(x) for x in v)
        if isinstance(v, dict):
            return 1 + self._count(v.get("params", [])) + self._count(v.get("statements", []))
        return 0

    def _parse_function(self, f):
        try:
            data = json.loads(f.get("content") or "[[]]")
        except ValueError:
            data = [[]]
        head = data[0][0] if data and data[0] else None
        d = {"id": f["id"], "value": f.get("type") == "value", "params": [], "stmts": [], "expr": None,
             "locals": {l["id"]: l.get("value", 0) for l in f.get("localVariables", [])}, "name": ""}
        if head:
            lab = head["params"][0]
            while isinstance(lab, dict):
                ps = lab.get("params", [])
                if lab["type"] == "function_field_label":
                    d["name"] = ps[0] if ps and isinstance(ps[0], str) else d["name"]
                    lab = ps[1] if len(ps) > 1 else None
                elif lab["type"] in ("function_field_string", "function_field_boolean"):
                    if ps and isinstance(ps[0], dict):
                        d["params"].append(ps[0]["type"].split("_", 1)[1])
                    lab = ps[1] if len(ps) > 1 else None
                else:
                    break
            st = head.get("statements") or [[]]
            d["stmts"] = (st[0] if st else []) + data[0][1:]
            if d["value"] and len(head["params"]) > 3:
                d["expr"] = head["params"][3]
        return d

    def reset(self):
        p = self.project
        self.t = 0.0
        self.frame = 0
        self.vars = {}
        self.lists = {}
        self.var_names = {}
        self.list_names = {}
        for v in self.var_defs:
            if v.get("object"):
                continue
            if v.get("variableType") == "list":
                self.lists[v["id"]] = [i.get("data") for i in v.get("array", [])]
                self.list_names[v["id"]] = v["name"]
            else:
                self.vars[v["id"]] = v.get("value", 0)
                self.var_names[v["id"]] = v["name"]
        for v in self.var_defs:
            if v.get("object"):
                if v.get("variableType") == "list":
                    self.list_names.setdefault(v["id"], v["name"])
                else:
                    self.var_names.setdefault(v["id"], v["name"])
        for f in self.fdefs.values():
            for lid in f["locals"]:
                pass
        for f in p.get("functions", []):
            for l in f.get("localVariables", []):
                self.var_names[l["id"]] = l["name"]
        self.visibility = {v["id"]: bool(v.get("visible")) for v in self.var_defs}
        self.tables = {t["id"]: {"name": t.get("name"), "fields": list(t.get("fields", [])),
                                 "data": [list(r) for r in t.get("data", [])]} for t in p.get("tables", [])}
        self.objects = []
        for i, o in enumerate(p.get("objects", [])):
            self.objects.append(Obj(o, self.scene_ids[0], self.var_defs))
        self.by_id = {o.id: o for o in self.objects}
        self.scene = self.scene_ids[0]
        self.threads = []
        self._clone_list = []
        self.timer = {"running": False, "start": 0.0, "acc": 0.0}
        self.answer = ""
        self.question = None
        self.answers = list(self.scenario.answers)
        self.pending_ask = None
        self.keys = set()
        self.mouse = {"x": to_num(self.scenario.mouse.get("x", 0)), "y": to_num(self.scenario.mouse.get("y", 0)), "down": False}
        self.clicked_obj = None
        self.events_q = list(self.scenario.events)
        self.log = []
        self.snapshots = []
        self.steps = 0
        self.stopped = None
        self.error = None
        self.next_every = {}
        for i, s in enumerate(self.dump_specs):
            if s[0] == "every":
                self.next_every[i] = s[1]
        self.clone_count = 0
        self._start()

    def _start(self):
        self._fire("when_run_button_click")
        self._fire("when_scene_start")

    # ---------- dump triggers ----------
    @staticmethod
    def _parse_spec(s):
        if isinstance(s, tuple):
            return s
        s = str(s).strip()
        if "=" in s:
            k, v = s.split("=", 1)
        elif ":" in s:
            k, v = s.split(":", 1)
        else:
            k, v = s, None
        k = k.strip().lower()
        if k in ("every", "at", "frame"):
            try:
                return (k, float(v))
            except (TypeError, ValueError):
                raise EntrypomError("BAD_ARGUMENT", "--dump-on %s 는 숫자 값이 필요합니다. 예: %s=1.5" % (k, k))
        if k in ("end", "error", "say", "ask", "scene", "clone", "stop"):
            return (k, v)
        if k in ("signal", "var", "list", "key", "click"):
            return (k, v)
        raise EntrypomError("BAD_ARGUMENT", "알 수 없는 --dump-on 트리거: %s (end, error, stop, every=초, at=초, frame=N, signal[=이름], var[=이름], list[=이름], say, ask, scene, clone, key[=이름], click)" % s)

    def emit(self, kind, **info):
        entry = {"t": round(self.t, 6), "frame": self.frame, "event": kind}
        entry.update(info)
        if len(self.log) < 5000:
            self.log.append(entry)
        for spec in self.dump_specs:
            k, v = spec
            if k == kind and (v is None or v == "*" or str(v) == str(info.get("name", info.get("text", info.get("key", ""))))):
                self.snapshot("%s%s" % (kind, "" if v is None else ":" + str(v)))
                break
        if kind == "signal":
            name = str(info.get("name", ""))
            if name == "dump" or name == "덤프" or name.startswith("dump:") or name.startswith("덤프:"):
                if not any(s[0] == "signal" for s in self.dump_specs):
                    self.snapshot("signal:" + name)

    def snapshot(self, trigger):
        if len(self.snapshots) >= 10000:
            return None
        o = self.snap_opts
        snap = {"trigger": trigger, "time": round(self.t, 6), "frame": self.frame}
        names = o["vars"]
        snap["variables"] = {self.var_names.get(k, k): out_value(v) for k, v in self.vars.items()
                             if names is None or self.var_names.get(k) in names}
        ln = o["lists"]
        snap["lists"] = {self.list_names.get(k, k): [out_value(x) for x in v] for k, v in self.lists.items()
                         if ln is None or self.list_names.get(k) in ln}
        if o["locals"]:
            loc = {}
            for ob in self.objects + [c for c in self.clones_iter()]:
                if ob.vars or ob.lists:
                    key = ob.name + ("#clone" if ob.is_clone else "")
                    d = {"variables": {self.var_names.get(k, k): out_value(v) for k, v in ob.vars.items()
                                       if names is None or self.var_names.get(k) in names},
                         "lists": {self.list_names.get(k, k): [out_value(x) for x in v] for k, v in ob.lists.items()
                                   if ln is None or self.list_names.get(k) in ln}}
                    loc.setdefault(key, []).append(d)
            snap["locals"] = {k: (v[0] if len(v) == 1 else v) for k, v in loc.items()}
        if o["tables"] and self.tables:
            snap["tables"] = {t["name"]: {"fields": t["fields"], "rows": t["data"]} for t in self.tables.values()}
        if o["objects"]:
            snap["objects"] = [self.obj_state(ob) for ob in self.objects + list(self.clones_iter())]
        self.snapshots.append(snap)
        return snap

    def clones_iter(self):
        return self.__dict__.get("_clone_list", [])

    def obj_state(self, ob):
        d = {"name": ob.name, "x": out_value(ob.x), "y": out_value(ob.y), "angle": out_value(ob.rotation),
             "way": out_value(ob.direction), "size": out_value(round(ob.size, 6)), "visible": ob.visible,
             "clone": ob.is_clone}
        if ob.pictures:
            d["costume"] = ob.pictures[ob.pic].get("name")
        if ob.text is not None:
            d["text"] = ob.text
        if ob.dialog:
            d["dialog"] = ob.dialog[0]
        return d

    # ---------- objects ----------
    def all_objects(self):
        return self.objects + self.__dict__.setdefault("_clone_list", [])

    def active(self, ob):
        return ob.alive and (ob.scene == self.scene or ob.scene is None)

    def find_targets(self, tid):
        return [o for o in self.all_objects() if o.id == tid and self.active(o)]

    # ---------- threads ----------
    def _spawn(self, ob, stmts, kind, tag=None):
        th = Thread(ob, kind, tag)
        th.gen = self._run_top(th, stmts)
        self.threads.append(th)
        return th

    def _run_top(self, th, stmts):
        try:
            yield from self.run_stmts(th, stmts)
        except self.StopThread:
            pass
        except (self.Break, self.Continue):
            pass

    def _fire(self, head_type, match=None, only=None):
        started = []
        for ob in self.all_objects():
            if not self.active(ob) or (only is not None and ob is not only):
                continue
            for head, body in self.scripts.get(ob.id, []):
                if head["type"] != head_type:
                    continue
                if match is not None and not match(head):
                    continue
                started.append(self._spawn(ob, body, head_type))
        return started

    # ---------- main loop ----------
    def step(self):
        self._apply_events()
        i = 0
        self.steps_frame = 0
        while i < len(self.threads):
            th = self.threads[i]
            i += 1
            if th.dead:
                continue
            try:
                next(th.gen)
            except StopIteration:
                th.dead = True
            except self.StopAll:
                self.stopped = "stop"
                return
            except RecursionError:
                raise self._err("STACK_OVERFLOW", "함수 호출이 너무 깊습니다(무한 재귀?).", th)
        self.threads = [t for t in self.threads if not t.dead]
        self._check_timers()
        self.t += self.dt
        self.frame += 1

    def _check_timers(self):
        for i, spec in enumerate(self.dump_specs):
            k, v = spec
            if k == "every" and self.t + 1e-9 >= self.next_every[i]:
                self.next_every[i] += v
                self.snapshot("every=%g" % v)
            elif k == "at" and not self.next_every.get(("done", i)) and self.t + 1e-9 >= v:
                self.next_every[("done", i)] = True
                self.snapshot("at=%g" % v)
            elif k == "frame" and self.frame == int(v):
                self.snapshot("frame=%d" % int(v))

    def _apply_events(self):
        while self.events_q and self.events_q[0]["t"] <= self.t + 1e-9:
            e = self.events_q.pop(0)
            do = e["do"]
            if do in ("key_down", "press"):
                code = e["code"]
                self.keys.add(code)
                self.emit("key", key=e.get("key"), name=e.get("key"))
                self._fire("when_some_key_pressed", lambda h, c=code: str(h["params"][1]) == str(c))
            elif do == "key_up":
                self.keys.discard(e["code"])
            elif do == "mouse_move":
                self.mouse["x"], self.mouse["y"] = to_num(e.get("x", 0)), to_num(e.get("y", 0))
            elif do == "mouse_down":
                if "x" in e:
                    self.mouse["x"], self.mouse["y"] = to_num(e["x"]), to_num(e.get("y", 0))
                self._mouse_down()
            elif do == "mouse_up":
                self._mouse_up()
            elif do == "click":
                if "x" in e:
                    self.mouse["x"], self.mouse["y"] = to_num(e["x"]), to_num(e.get("y", 0))
                self._mouse_down()
                self._mouse_up()
            elif do == "answer":
                self.answers.append(to_str(e.get("text", "")))
            elif do == "signal":
                self.cast(self.msg_ids.get(e.get("name"), e.get("name")))
            else:
                raise EntrypomError("BAD_SCENARIO", "알 수 없는 입력 종류: %r" % do)

    def _hit_object(self):
        for ob in reversed(self.z_order()):
            if self.active(ob) and ob.visible and point_in(ob.corners(), self.mouse["x"], self.mouse["y"]):
                return ob
        return None

    def z_order(self):
        return self.all_objects()

    def _mouse_down(self):
        self.mouse["down"] = True
        self.emit("click", name="down", x=self.mouse["x"], y=self.mouse["y"])
        self._fire("mouse_clicked")
        ob = self._hit_object()
        self.clicked_obj = ob
        if ob is not None:
            self._fire("when_object_click", only=ob)

    def _mouse_up(self):
        self.mouse["down"] = False
        self._fire("mouse_click_cancled")
        if self.clicked_obj is not None:
            self._fire("when_object_click_canceled", only=self.clicked_obj)
        self.clicked_obj = None

    def finished(self):
        if self.stopped:
            return self.stopped
        if not self.threads and not self.events_q:
            return "finished"
        if not self.threads and self.events_q:
            return None
        return None

    def run(self):
        """Runs until finish/stop/timeout. Returns the result dict (errors included)."""
        limit_frames = self.max_frames if self.max_frames is not None else int(self.max_seconds * self.fps)
        reason = None
        try:
            while True:
                reason = self.finished()
                if reason:
                    break
                if self.frame >= limit_frames:
                    reason = "timeout"
                    break
                self.step()
                if self.waiting_ask_forever():
                    raise self._err("NO_ANSWER", "ask 블록이 대답을 기다리지만 준비된 대답이 없습니다. --answer 로 대답을 지정하세요.", None)
        except SimError as e:
            self.error = e
            reason = "error"
            self.emit("error", name=e.code, message=e.message)
        if reason == "stop":
            self.emit("stop")
        self.emit("end", name=reason)
        return self.result(reason)

    def waiting_ask_forever(self):
        return self.pending_ask is not None and not self.answers and not any(e["do"] in ("answer",) for e in self.events_q)

    def result(self, reason):
        final = self.snapshot("final") if True else None
        res = {"ok": self.error is None, "reason": reason, "time": round(self.t, 6), "frames": self.frame,
               "fps": self.fps}
        if self.error is not None:
            res["error"] = self.error.to_dict()
        res["final"] = {k: final[k] for k in final if k not in ("trigger",)}
        self.snapshots.pop()
        res["snapshots"] = self.snapshots
        res["events"] = [e for e in self.log if e["event"] in ("say", "ask", "signal", "scene", "error", "stop", "end", "clone")][:1000]
        res["warnings"] = self.warnings
        return res

    # ---------- errors ----------
    def _err(self, code, message, th=None, **kw):
        d = {"time": round(self.t, 6), "frame": self.frame}
        if th is not None:
            d["object"] = th.obj.name
            d["script"] = th.kind
        d.update(kw)
        return SimError(code, message, **d)


HEADS = {"when_run_button_click", "when_scene_start", "when_some_key_pressed", "when_object_click",
         "when_object_click_canceled", "mouse_clicked", "mouse_click_cancled", "when_message_cast",
         "when_clone_start", "function_create", "function_create_value", "function_field_label",
         "function_field_string", "function_field_boolean"}


def static_check(project):
    """Lists block types used by the project that Entrypom cannot run."""
    found = {}

    def walk(v, where):
        if isinstance(v, list):
            for x in v:
                walk(x, where)
        elif isinstance(v, dict):
            t = v.get("type")
            if t:
                ok = (t in HEADS or t.startswith(("stringParam_", "booleanParam_", "func_"))
                      or hasattr(Blocks, "s_" + t) or hasattr(Blocks, "e_" + t) or t in Blocks.NOOP)
                if not ok:
                    found.setdefault(t, set()).add(where)
            walk(v.get("params"), where)
            walk(v.get("statements"), where)

    for o in project.get("objects", []):
        try:
            walk(json.loads(o.get("script") or "[]"), o.get("name"))
        except ValueError:
            pass
    for f in project.get("functions", []):
        try:
            walk(json.loads(f.get("content") or "[]"), "function:" + f["id"])
        except ValueError:
            pass
    return [{"block": k, "where": sorted(v)} for k, v in sorted(found.items())]
