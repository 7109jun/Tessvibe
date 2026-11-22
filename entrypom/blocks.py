"""Entry block implementations (statements `s_*`, expressions `e_*`)."""
import datetime
import math
import re

from .project import EntrypomError
from .values import compare, js_round, norm, parse_number, to_num, to_str, truthy

HANGUL = re.compile(r"^[ㄱ-ㅎㅏ-ㅣ가-힣\s]+$")
ENGLISH = re.compile(r"^[A-Za-z\s]+$")
STUB_TEXT = {"get_cur_weather", "get_cur_wind", "get_today_temperature", "get_weather_data", "check_weather",
             "check_finedust", "check_disaster_alert", "get_disaster_alert", "count_disaster_guideline",
             "get_disaster_guideline", "count_festival", "get_festival_info", "get_translated_string",
             "get_korea_area_code"}


def store(v):
    """Value as stored in variables/lists (booleans become TRUE/FALSE like Entry)."""
    if isinstance(v, bool):
        return "TRUE" if v else "FALSE"
    return v


class Blocks:
    class Break(Exception):
        pass

    class Continue(Exception):
        pass

    class StopThread(Exception):
        pass

    class StopAll(Exception):
        pass

    # ================= infrastructure =================
    def p(self, th, b, i):
        ps = b.get("params", [])
        if i >= len(ps):
            return None
        return self.ev(th, ps[i])

    def ev(self, th, x):
        if not isinstance(x, dict):
            return x
        t = x["type"]
        h = getattr(self, "e_" + t, None)
        if h is not None:
            try:
                return h(th, x)
            except EntrypomError as e:
                self._tag(e, th, x)
                raise
        if t.startswith("stringParam_") or t.startswith("booleanParam_"):
            return self._param(th, x, t.split("_", 1)[1])
        if t.startswith("func_"):
            return self._call_value(th, x, t[5:])
        raise self._err("UNSUPPORTED_BLOCK", "지원하지 않는 값 블록: %s" % t, th, block=t, block_id=x.get("id"))

    def _tag(self, e, th, b):
        d = e.detail
        d.setdefault("block", b.get("type"))
        d.setdefault("block_id", b.get("id"))
        if th is not None:
            d.setdefault("object", th.obj.name)
            d.setdefault("script", th.kind)

    def run_stmts(self, th, stmts):
        for b in stmts:
            self.steps += 1
            if self.steps > self.step_limit:
                raise self._err("STEP_LIMIT", "실행 블록 수가 한도(%d)를 넘었습니다." % self.step_limit, th, block=b["type"])
            t = b["type"]
            h = getattr(self, "s_" + t, None)
            try:
                if h is None:
                    if t.startswith("func_"):
                        r = self._call(th, b, t[5:])
                    elif t in self.NOOP:
                        self._noop(th, b)
                        continue
                    else:
                        raise self._err("UNSUPPORTED_BLOCK", "지원하지 않는 블록: %s" % t, th, block=t, block_id=b.get("id"))
                else:
                    r = h(th, b)
                if r is not None:
                    yield from r
            except EntrypomError as e:
                self._tag(e, th, b)
                raise
            except (self.Break, self.Continue, self.StopThread, self.StopAll, GeneratorExit):
                raise
            except RecursionError:
                raise
            except Exception as e:  # internal failure: report as JSON, not a traceback
                raise self._err("INTERNAL_ERROR", "%s: %s" % (type(e).__name__, e), th, block=t, block_id=b.get("id"))

    def _noop(self, th, b):
        self.noop_count = getattr(self, "noop_count", 0) + 1
        if b["type"] in STUB_TEXT:
            self._warn("STUBBED_BLOCK", "네트워크/외부 의존 블록 '%s' 은 시뮬레이터에서 빈 값을 냅니다." % b["type"])

    NOOP = {
        "brush_erase_all", "brush_stamp", "start_drawing", "stop_drawing", "start_fill", "stop_fill",
        "change_brush_transparency", "set_brush_tranparency", "change_thickness", "set_thickness", "set_color",
        "set_fill_color", "set_random_color", "sound_something_with_block", "sound_something_second_with_block",
        "sound_from_to", "sound_silent_all", "sound_speed_set", "sound_volume_change", "sound_volume_set",
        "play_bgm", "stop_bgm", "read_text", "set_tts_property", "open_table", "open_table_chart", "close_table_chart",
        "save_current_table", "text_change_bg_color", "text_change_effect", "text_change_font",
        "text_change_font_color", "check_weather", "check_finedust", "check_disaster_alert",
    }

    def _wait(self, secs):
        end = self.t + secs
        yield
        while self.t < end - 1e-9:
            yield

    # ================= lookup helpers =================
    def _var_scope(self, th, vid):
        o = th.obj if th else None
        if o is not None and vid in o.vars:
            return o.vars
        if vid in self.vars:
            return self.vars
        if th is not None and th.frames and vid in th.frames[-1]["locals"]:
            return th.frames[-1]["locals"]
        raise self._err("UNKNOWN_VARIABLE", "변수 id '%s' 를 찾을 수 없습니다." % vid, th)

    def _list(self, th, lid):
        o = th.obj if th else None
        if o is not None and lid in o.lists:
            return o.lists[lid]
        if lid in self.lists:
            return self.lists[lid]
        raise self._err("UNKNOWN_LIST", "리스트 id '%s' 를 찾을 수 없습니다." % lid, th)

    def _set_var(self, th, vid, val):
        scope = self._var_scope(th, vid)
        val = store(val)
        if isinstance(val, float) and (val != val or val in (math.inf, -math.inf)):
            self._warn("NON_FINITE_VALUE", "변수 '%s' 에 %s 가 저장되었습니다(0으로 나눗셈?)." % (self.var_names.get(vid, vid), to_str(val)))
        old = scope.get(vid)
        scope[vid] = val
        if old != val or type(old) != type(val):
            self.emit("var", name=self.var_names.get(vid, vid), value=val)

    def _list_changed(self, lid):
        self.emit("list", name=self.list_names.get(lid, lid))

    def _idx(self, th, lst, v, what="리스트", allow_end=False):
        n = parse_number(v)
        if n is None or n != int(n):
            raise self._err("BAD_INDEX", "%s 번호가 정수가 아닙니다: %r" % (what, v), th)
        n = int(n)
        top = len(lst) + (1 if allow_end else 0)
        if n < 1 or n > top:
            raise self._err("INDEX_OUT_OF_RANGE", "%s 번호 %d 가 범위(1~%d)를 벗어났습니다." % (what, n, top), th, index=n, length=len(lst))
        return n - 1

    def _target_objs(self, th, t):
        if t in (None, "self"):
            return [th.obj]
        r = self.find_targets(t)
        return r

    def _target_pos(self, th, t):
        if t == "mouse":
            return self.mouse["x"], self.mouse["y"]
        r = self._target_objs(th, t)
        if not r:
            raise self._err("UNKNOWN_OBJECT", "대상 오브젝트 '%s' 를 찾을 수 없습니다." % t, th)
        return r[0].x, r[0].y

    # ================= functions =================
    def _frame(self, th, fid, b):
        f = self.fdefs.get(fid)
        if f is None:
            raise self._err("UNKNOWN_FUNCTION", "함수 '%s' 를 찾을 수 없습니다." % fid, th)
        args = {}
        for i, pid in enumerate(f["params"]):
            args[pid] = self.p(th, b, i)
        return f, {"args": args, "locals": dict(f["locals"])}

    def _param(self, th, x, pid):
        if th.frames and pid in th.frames[-1]["args"]:
            return th.frames[-1]["args"][pid]
        raise self._err("UNKNOWN_PARAM", "함수 인자 '%s' 를 현재 함수 밖에서 썼습니다." % pid, th)

    def _call(self, th, b, fid):
        f, fr = self._frame(th, fid, b)
        return self._invoke(th, f, fr)

    def _invoke(self, th, f, fr):
        if len(th.frames) > 400:
            raise self._err("STACK_OVERFLOW", "함수 호출이 너무 깊습니다(무한 재귀?).", th)
        th.frames.append(fr)
        try:
            try:
                yield from self.run_stmts(th, f["stmts"])
            except self.StopThread:
                raise
            if f["expr"] is not None:
                fr["result"] = self.ev(th, f["expr"])
        finally:
            th.frames.pop()

    def _call_value(self, th, b, fid):
        f, fr = self._frame(th, fid, b)
        gen = self._invoke(th, f, fr)
        for _ in gen:
            pass
        return fr.get("result", 0)

    def s_function_create(self, th, b):
        return None

    # ================= control =================
    def s_wait_second(self, th, b):
        return self._wait(to_num(self.p(th, b, 0)))

    def s_wait_until_true(self, th, b):
        return self._wait_until(th, b)

    def _wait_until(self, th, b):
        while not truthy(self.p(th, b, 0)):
            yield

    def _loop(self, th, b, cond):
        body = (b.get("statements") or [[]])[0]
        while cond():
            try:
                yield from self.run_stmts(th, body)
            except self.Continue:
                pass
            except self.Break:
                break
            yield

    def s_repeat_basic(self, th, b):
        n = to_num(self.p(th, b, 0))
        if n != n or n == math.inf:
            n = 0 if n != n else 10 ** 12
        state = {"i": 0, "n": int(n)}

        def cond():
            state["i"] += 1
            return state["i"] <= state["n"]
        return self._loop(th, b, cond)

    def s_repeat_inf(self, th, b):
        return self._loop(th, b, lambda: True)

    def s_repeat_while_true(self, th, b):
        mode = self.p(th, b, 1)
        if mode == "until":
            return self._loop(th, b, lambda: not truthy(self.p(th, b, 0)))
        return self._loop(th, b, lambda: truthy(self.p(th, b, 0)))

    def s_stop_repeat(self, th, b):
        raise self.Break()

    def s_continue_repeat(self, th, b):
        raise self.Continue()

    def s__if(self, th, b):
        if truthy(self.p(th, b, 0)):
            return self.run_stmts(th, (b.get("statements") or [[]])[0])

    def s_if_else(self, th, b):
        st = b.get("statements") or [[], []]
        if truthy(self.p(th, b, 0)):
            return self.run_stmts(th, st[0])
        return self.run_stmts(th, st[1] if len(st) > 1 else [])

    def s_stop_object(self, th, b):
        k = self.p(th, b, 0)
        if k == "all":
            raise self.StopAll()
        if k == "thisOnly":
            raise self.StopThread()
        for t in self.threads:
            if t.obj is th.obj and (k == "thisObject" or (k == "otherThread" and t is not th)):
                t.dead = True
        if k == "thisObject":
            raise self.StopThread()

    def s_restart_project(self, th, b):
        self._restart = True
        self.emit("restart")
        t0 = self.t
        self.reset_keep_time()

    def reset_keep_time(self):
        t, f, snaps, log, warn = self.t, self.frame, self.snapshots, self.log, self.warnings
        evq = self.events_q
        self.reset()
        self.t, self.frame, self.snapshots, self.log, self.warnings = t, f, snaps, log, warn
        self.events_q = evq
        raise self.StopThread()

    def s_start_scene(self, th, b):
        self._goto_scene(self.p(th, b, 0))

    def s_start_neighbor_scene(self, th, b):
        k = self.p(th, b, 0)
        i = self.scene_ids.index(self.scene)
        j = (i + 1) % len(self.scene_ids) if k == "next" else (i - 1) % len(self.scene_ids)
        self._goto_scene(self.scene_ids[j])

    def _goto_scene(self, sid):
        if sid not in self.scene_ids:
            raise self._err("UNKNOWN_SCENE", "장면 '%s' 를 찾을 수 없습니다." % sid)
        self.scene = sid
        for t in self.threads:
            t.dead = True
        self.__dict__["_clone_list"] = []
        self.emit("scene", name=self.scene_names.get(sid, sid))
        self._fire("when_scene_start")
        raise self.StopThread()

    # ================= signals =================
    def cast(self, mid):
        name = self.msg_names.get(mid, mid)
        self.emit("signal", name=name)
        return self._fire("when_message_cast", lambda h: h["params"][1] == mid)

    def s_message_cast(self, th, b):
        self.cast(self.p(th, b, 0))

    def s_message_cast_wait(self, th, b):
        started = self.cast(self.p(th, b, 0))
        return self._join(started)

    def _join(self, started):
        yield
        while any(not t.dead for t in started):
            yield

    # ================= clones =================
    def s_create_clone(self, th, b):
        t = self.p(th, b, 0)
        for src in self._target_objs(th, t)[:1]:
            if len(self.__dict__.setdefault("_clone_list", [])) >= 1000:
                raise self._err("CLONE_LIMIT", "복제본이 1000개를 넘었습니다.", th)
            c = src.clone_of(None)
            self._clone_list.append(c)
            self.emit("clone", name=c.name)
            self._fire("when_clone_start", only=c)

    def _kill(self, ob):
        ob.alive = False
        for t in self.threads:
            if t.obj is ob:
                t.dead = True

    def s_delete_clone(self, th, b):
        if th.obj.is_clone:
            self._kill(th.obj)
            self._clone_list = [c for c in self._clone_list if c.alive]
            raise self.StopThread()

    def s_remove_all_clones(self, th, b):
        for c in list(self.__dict__.get("_clone_list", [])):
            self._kill(c)
        self._clone_list = []
        if th.obj.is_clone:
            raise self.StopThread()

    # ================= movement =================
    def s_move_x(self, th, b):
        th.obj.x += to_num(self.p(th, b, 0))

    def s_move_y(self, th, b):
        th.obj.y += to_num(self.p(th, b, 0))

    def s_move_direction(self, th, b):
        o, d = th.obj, to_num(self.p(th, b, 0))
        r = math.radians(o.direction)
        o.x += d * math.sin(r)
        o.y += d * math.cos(r)

    def s_move_to_angle(self, th, b):
        a, d = to_num(self.p(th, b, 0)), to_num(self.p(th, b, 1))
        r = math.radians(a)
        th.obj.x += d * math.sin(r)
        th.obj.y += d * math.cos(r)

    def s_locate_x(self, th, b):
        th.obj.x = to_num(self.p(th, b, 0))

    def s_locate_y(self, th, b):
        th.obj.y = to_num(self.p(th, b, 0))

    def s_locate_xy(self, th, b):
        th.obj.x, th.obj.y = to_num(self.p(th, b, 0)), to_num(self.p(th, b, 1))

    def s_locate(self, th, b):
        th.obj.x, th.obj.y = self._target_pos(th, self.p(th, b, 0))

    def _glide(self, o, tx, ty, secs):
        x0, y0 = o.x, o.y
        end = self.t + secs
        n = max(1, int(round(secs * self.fps)))
        start = self.t
        yield
        while self.t < end - 1e-9:
            k = min(1.0, (self.t - start) / secs) if secs > 0 else 1.0
            o.x, o.y = x0 + (tx - x0) * k, y0 + (ty - y0) * k
            yield
        o.x, o.y = tx, ty

    def s_move_xy_time(self, th, b):
        s, dx, dy = to_num(self.p(th, b, 0)), to_num(self.p(th, b, 1)), to_num(self.p(th, b, 2))
        return self._glide(th.obj, th.obj.x + dx, th.obj.y + dy, s)

    def s_locate_object_time(self, th, b):
        s = to_num(self.p(th, b, 0))
        tx, ty = self._target_pos(th, self.p(th, b, 1))
        return self._glide(th.obj, tx, ty, s)

    def s_bounce_wall(self, th, b):
        o = th.obj
        xs = [c[0] for c in o.corners()]
        ys = [c[1] for c in o.corners()]
        r = math.radians(o.direction)
        dx, dy = math.sin(r), math.cos(r)
        if max(ys) >= 135 and dy > 0 or min(ys) <= -135 and dy < 0:
            dy = -dy
        if max(xs) >= 240 and dx > 0 or min(xs) <= -240 and dx < 0:
            dx = -dx
        o.direction = math.degrees(math.atan2(dx, dy)) % 360

    # ================= rotation =================
    @staticmethod
    def _ang(v):
        v = v % 360
        return norm(v)

    def s_rotate_absolute(self, th, b):
        th.obj.rotation = self._ang(to_num(self.p(th, b, 0)))

    def s_rotate_relative(self, th, b):
        th.obj.rotation = self._ang(th.obj.rotation + to_num(self.p(th, b, 0)))

    def s_direction_absolute(self, th, b):
        th.obj.direction = self._ang(to_num(self.p(th, b, 0)))

    def s_direction_relative(self, th, b):
        th.obj.direction = self._ang(th.obj.direction + to_num(self.p(th, b, 0)))

    def s_rotate_by_time(self, th, b):
        return self._turn(th.obj, "rotation", to_num(self.p(th, b, 0)), to_num(self.p(th, b, 1)))

    def s_direction_relative_duration(self, th, b):
        return self._turn(th.obj, "direction", to_num(self.p(th, b, 0)), to_num(self.p(th, b, 1)))

    def _turn(self, o, attr, secs, delta):
        a0 = getattr(o, attr)
        start, end = self.t, self.t + secs
        yield
        while self.t < end - 1e-9:
            setattr(o, attr, self._ang(a0 + delta * (self.t - start) / secs))
            yield
        setattr(o, attr, self._ang(a0 + delta))

    def s_see_angle_object(self, th, b):
        tx, ty = self._target_pos(th, self.p(th, b, 0))
        o = th.obj
        a = math.degrees(math.atan2(tx - o.x, ty - o.y)) % 360
        o.rotation = o.direction = norm(a)

    # ================= looks =================
    def s_show(self, th, b):
        th.obj.visible = True

    def s_hide(self, th, b):
        th.obj.visible = False

    def _say(self, th, text, mode):
        th.obj.dialog = (to_str(text), mode)
        self.emit("say", name=th.obj.name, text=to_str(text), mode=mode)

    def s_dialog(self, th, b):
        self._say(th, self.p(th, b, 0), self.p(th, b, 1))

    def s_dialog_time(self, th, b):
        self._say(th, self.p(th, b, 0), self.p(th, b, 2))
        return self._dialog_wait(th, to_num(self.p(th, b, 1)))

    def _dialog_wait(self, th, secs):
        yield from self._wait(secs)
        th.obj.dialog = None

    def s_remove_dialog(self, th, b):
        th.obj.dialog = None

    def s_change_to_next_shape(self, th, b):
        o = th.obj
        if o.pictures:
            step = 1 if self.p(th, b, 0) == "next" else -1
            self._set_pic(o, (o.pic + step) % len(o.pictures))

    def s_change_to_some_shape(self, th, b):
        o, v = th.obj, self.p(th, b, 0)
        for i, pic in enumerate(o.pictures):
            if pic.get("id") == v or pic.get("name") == v:
                return self._set_pic(o, i)
        n = parse_number(v)
        if n is not None and 1 <= n <= len(o.pictures):
            return self._set_pic(o, int(n) - 1)
        raise self._err("UNKNOWN_COSTUME", "모양 '%s' 를 찾을 수 없습니다." % v, th)

    def _set_pic(self, o, i):
        o.pic = i
        d = o.pictures[i].get("dimension") or {}
        if d.get("width") and d.get("height"):
            o.w, o.h = d["width"], d["height"]

    def _set_size(self, o, v):
        v = max(v, 1)
        cur = o.size
        if cur == 0:
            o.sx = o.sy = v / ((o.w + o.h) / 2 or 1)
            return
        k = v / cur
        o.sx *= k
        o.sy *= k

    def s_set_scale_size(self, th, b):
        self._set_size(th.obj, to_num(self.p(th, b, 0)))

    def s_change_scale_size(self, th, b):
        self._set_size(th.obj, th.obj.size + to_num(self.p(th, b, 0)))

    def s_stretch_scale_size(self, th, b):
        o, axis, v = th.obj, self.p(th, b, 0), to_num(self.p(th, b, 1))
        cur = o.w * abs(o.sx) if axis == "WIDTH" else o.h * abs(o.sy)
        if cur:
            k = (cur + v) / cur
            if axis == "WIDTH":
                o.sx *= k
            else:
                o.sy *= k

    def s_reset_scale_size(self, th, b):
        th.obj.sx, th.obj.sy = th.obj.init_sx, th.obj.init_sy

    def s_flip_x(self, th, b):
        th.obj.sx = -th.obj.sx

    def s_flip_y(self, th, b):
        th.obj.sy = -th.obj.sy

    def s_change_object_index(self, th, b):
        lst = self.objects
        o = th.obj
        if o not in lst:
            return
        mode, i = self.p(th, b, 0), lst.index(o)
        lst.remove(o)
        if mode == "FRONT":
            lst.append(o)
        elif mode == "BACK":
            lst.insert(0, o)
        elif mode == "FORWARD":
            lst.insert(min(i + 1, len(lst)), o)
        else:
            lst.insert(max(i - 1, 0), o)

    def s_add_effect_amount(self, th, b):
        k = self.p(th, b, 0)
        th.obj.effects[k] = th.obj.effects.get(k, 0) + to_num(self.p(th, b, 1))

    s_change_effect_amount = s_add_effect_amount

    def s_erase_all_effects(self, th, b):
        th.obj.effects.clear()

    # ================= text box =================
    def _txt(self, th):
        if th.obj.text is None:
            raise self._err("NOT_TEXT_BOX", "글상자가 아닌 오브젝트에 글상자 블록을 썼습니다.", th)
        return th.obj

    def s_text_write(self, th, b):
        self._txt(th).text = to_str(self.p(th, b, 0))

    def s_text_append(self, th, b):
        o = self._txt(th)
        o.text = to_str(o.text) + to_str(self.p(th, b, 0))

    def s_text_prepend(self, th, b):
        o = self._txt(th)
        o.text = to_str(self.p(th, b, 0)) + to_str(o.text)

    def s_text_flush(self, th, b):
        self._txt(th).text = ""

    def e_text_read(self, th, b):
        t = self.p(th, b, 0)
        objs = self._target_objs(th, t) if t not in (None, "self") else [th.obj]
        for o in objs:
            if o.text is not None:
                return o.text
        raise self._err("NOT_TEXT_BOX", "글상자 '%s' 를 찾을 수 없습니다." % t, th)

    # ================= variables & lists =================
    def s_set_variable(self, th, b):
        self._set_var(th, self.p(th, b, 0), self.p(th, b, 1))

    def s_change_variable(self, th, b):
        vid = self.p(th, b, 0)
        cur = self._var_scope(th, vid)[vid]
        val = self.p(th, b, 1)
        a, c = parse_number(cur), parse_number(val)
        if a is not None and c is not None:
            self._set_var(th, vid, norm(a + c))
        else:
            self._set_var(th, vid, to_str(cur) + to_str(val))

    def s_set_func_variable(self, th, b):
        vid = self.p(th, b, 0)
        th.frames[-1]["locals"][vid] = store(self.p(th, b, 1))

    def e_get_func_variable(self, th, b):
        vid = self.p(th, b, 0)
        if th.frames and vid in th.frames[-1]["locals"]:
            return th.frames[-1]["locals"][vid]
        raise self._err("UNKNOWN_VARIABLE", "함수 지역 변수 '%s' 를 찾을 수 없습니다." % vid, th)

    def e_get_variable(self, th, b):
        vid = self.p(th, b, 0)
        return self._var_scope(th, vid)[vid]

    def s_show_variable(self, th, b):
        self.visibility[self.p(th, b, 0)] = True

    def s_hide_variable(self, th, b):
        self.visibility[self.p(th, b, 0)] = False

    s_show_list = s_show_variable
    s_hide_list = s_hide_variable

    def s_add_value_to_list(self, th, b):
        lid = self.p(th, b, 1)
        self._list(th, lid).append(store(self.p(th, b, 0)))
        self._list_changed(lid)

    def s_remove_value_from_list(self, th, b):
        lid = self.p(th, b, 1)
        lst = self._list(th, lid)
        del lst[self._idx(th, lst, self.p(th, b, 0))]
        self._list_changed(lid)

    def s_insert_value_to_list(self, th, b):
        lid = self.p(th, b, 1)
        lst = self._list(th, lid)
        i = self._idx(th, lst, self.p(th, b, 2), allow_end=True)
        lst.insert(i, store(self.p(th, b, 0)))
        self._list_changed(lid)

    def s_change_value_list_index(self, th, b):
        lid = self.p(th, b, 0)
        lst = self._list(th, lid)
        lst[self._idx(th, lst, self.p(th, b, 1))] = store(self.p(th, b, 2))
        self._list_changed(lid)

    def e_value_of_index_from_list(self, th, b):
        lst = self._list(th, self.p(th, b, 1))
        return lst[self._idx(th, lst, self.p(th, b, 3))]

    def e_length_of_list(self, th, b):
        return len(self._list(th, self.p(th, b, 1)))

    def e_is_included_in_list(self, th, b):
        lst = self._list(th, self.p(th, b, 1))
        v = self.p(th, b, 3)
        return any(compare(x, "EQUAL", v) for x in lst)

    # ================= tables =================
    def _table(self, th, tid):
        t = self.tables.get(tid)
        if t is None:
            raise self._err("UNKNOWN_TABLE", "테이블 '%s' 를 찾을 수 없습니다." % tid, th)
        return t

    def _col(self, th, t, f):
        if f in t["fields"]:
            return t["fields"].index(f)
        n = parse_number(f)
        if n is not None and 1 <= n <= len(t["fields"]):
            return int(n) - 1
        raise self._err("UNKNOWN_FIELD", "테이블 '%s' 에 '%s' 열이 없습니다. (열: %s)" % (t["name"], f, ", ".join(t["fields"])), th)

    def _row(self, th, t, r):
        n = parse_number(r)
        if n is None or not 1 <= n <= len(t["data"]):
            raise self._err("INDEX_OUT_OF_RANGE", "테이블 '%s' 의 줄 %s 가 범위(1~%d)를 벗어났습니다." % (t["name"], r, len(t["data"])), th)
        return int(n) - 1

    def _cell(self, th, t, ref):
        m = re.match(r"^\s*([A-Za-z]+)\s*(\d+)\s*$", to_str(ref))
        if not m:
            raise self._err("BAD_CELL", "셀 주소가 잘못되었습니다: %r (예: B2)" % ref, th)
        col = 0
        for ch in m.group(1).upper():
            col = col * 26 + ord(ch) - 64
        col -= 1
        row = int(m.group(2))
        if row == 1 and 0 <= col < len(t["fields"]):
            return None, col
        if not (0 <= col < len(t["fields"])) or not 2 <= row <= len(t["data"]) + 1:
            raise self._err("INDEX_OUT_OF_RANGE", "셀 %s 가 테이블 '%s' 범위를 벗어났습니다." % (ref, t["name"]), th)
        return row - 2, col

    def e_get_value_from_table(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        return t["data"][self._row(th, t, self.p(th, b, 1))][self._col(th, t, self.p(th, b, 2))]

    def e_get_value_from_last_row(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        if not t["data"]:
            raise self._err("INDEX_OUT_OF_RANGE", "테이블 '%s' 에 줄이 없습니다." % t["name"], th)
        return t["data"][-1][self._col(th, t, self.p(th, b, 1))]

    def e_get_value_from_cell(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        r, c = self._cell(th, t, self.p(th, b, 1))
        return t["fields"][c] if r is None else t["data"][r][c]

    def e_get_table_count(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        return len(t["data"]) if self.p(th, b, 1) == "ROW" else len(t["fields"])

    def e_get_value_v_lookup(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        kc, key, vc = self._col(th, t, self.p(th, b, 1)), self.p(th, b, 2), self._col(th, t, self.p(th, b, 3))
        for row in t["data"]:
            if compare(row[kc], "EQUAL", key):
                return row[vc]
        raise self._err("NOT_FOUND", "테이블 '%s' 에서 '%s' 를 찾지 못했습니다." % (t["name"], key), th)

    def _column_nums(self, th, t, f):
        c = self._col(th, t, f)
        return [to_num(r[c]) for r in t["data"]]

    def e_calc_values_from_table(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        xs = self._column_nums(th, t, self.p(th, b, 1))
        op = self.p(th, b, 2)
        if not xs:
            return 0
        if op == "SUM":
            return norm(sum(xs))
        if op in ("AVG", "AVERAGE"):
            return norm(sum(xs) / len(xs))
        if op == "MAX":
            return max(xs)
        if op == "MIN":
            return min(xs)
        if op == "COUNT":
            return len(xs)
        raise self._err("UNSUPPORTED_BLOCK", "calc_values_from_table 연산 '%s' 는 지원하지 않습니다." % op, th)

    def e_get_coefficient(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        xs, ys = self._column_nums(th, t, self.p(th, b, 1)), self._column_nums(th, t, self.p(th, b, 2))
        n = len(xs)
        if n == 0:
            return 0
        mx, my = sum(xs) / n, sum(ys) / n
        sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
        d = math.sqrt(sum((x - mx) ** 2 for x in xs) * sum((y - my) ** 2 for y in ys))
        return norm(sxy / d) if d else 0

    def s_set_value_from_table(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        t["data"][self._row(th, t, self.p(th, b, 1))][self._col(th, t, self.p(th, b, 2))] = to_str(self.p(th, b, 3))
        self.emit("table", name=t["name"])

    def s_set_value_from_cell(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        r, c = self._cell(th, t, self.p(th, b, 1))
        v = to_str(self.p(th, b, 2))
        if r is None:
            t["fields"][c] = v
        else:
            t["data"][r][c] = v
        self.emit("table", name=t["name"])

    def s_append_row_to_table(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        if self.p(th, b, 1) == "COLUMN":
            t["fields"].append("")
            for r in t["data"]:
                r.append("")
        else:
            t["data"].append([""] * len(t["fields"]))
        self.emit("table", name=t["name"])

    def s_insert_row_to_table(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        n = parse_number(self.p(th, b, 1))
        if n is None or not 1 <= n <= len(t["data"]) + 1:
            raise self._err("INDEX_OUT_OF_RANGE", "삽입 위치 %s 가 범위를 벗어났습니다." % self.p(th, b, 1), th)
        t["data"].insert(int(n) - 1, [""] * len(t["fields"]))
        self.emit("table", name=t["name"])

    def s_delete_row_from_table(self, th, b):
        t = self._table(th, self.p(th, b, 0))
        del t["data"][self._row(th, t, self.p(th, b, 1))]
        self.emit("table", name=t["name"])

    def s_open_table_wait(self, th, b):
        return None

    # ================= input =================
    def s_ask_and_wait(self, th, b):
        q = to_str(self.p(th, b, 0))
        return self._ask(th, q)

    def _ask(self, th, q):
        self.question = q
        self.pending_ask = th
        self.emit("ask", name=th.obj.name, text=q)
        yield
        while not self.answers:
            yield
        self.answer = self.answers.pop(0)
        self.pending_ask = None
        self.question = None
        th.obj.dialog = None

    def e_get_canvas_input_value(self, th, b):
        return self.answer

    def s_set_visible_answer(self, th, b):
        pass

    def e_is_press_some_key(self, th, b):
        return int(to_num(self.p(th, b, 0))) in self.keys

    def e_is_clicked(self, th, b):
        return self.mouse["down"]

    e_mouse_clicked_value = e_is_clicked

    def e_is_object_clicked(self, th, b):
        return self.mouse["down"] and self.clicked_obj is th.obj

    def e_is_touch_supported(self, th, b):
        return False

    def e_is_boost_mode(self, th, b):
        return False

    def e_is_current_device_type(self, th, b):
        return self.p(th, b, 0) == "desktop"

    def e_coordinate_mouse(self, th, b):
        return self.mouse[self.p(th, b, 1)]

    def e_get_nickname(self, th, b):
        return self.scenario.user

    e_get_user_name = e_get_nickname

    def e_check_language(self, th, b):
        return False

    # ================= timer =================
    def s_choose_project_timer_action(self, th, b):
        a, tm = self.p(th, b, 1), self.timer
        if a == "START" and not tm["running"]:
            tm["running"], tm["start"] = True, self.t
        elif a == "STOP" and tm["running"]:
            tm["acc"] += self.t - tm["start"]
            tm["running"] = False
        elif a == "RESET":
            tm["acc"], tm["start"] = 0.0, self.t

    def s_set_visible_project_timer(self, th, b):
        pass

    def e_get_project_timer_value(self, th, b):
        tm = self.timer
        return norm(round(tm["acc"] + (self.t - tm["start"] if tm["running"] else 0), 6))

    # ================= sounds (silent) =================
    def _sound_len(self, th, ref):
        for s in th.obj.sounds:
            if s.get("id") == ref or s.get("name") == ref:
                return to_num(s.get("duration", 0))
        return 0

    def s_sound_something_wait_with_block(self, th, b):
        return self._wait(self._sound_len(th, self.p(th, b, 0)))

    def s_sound_something_second_wait_with_block(self, th, b):
        return self._wait(to_num(self.p(th, b, 1)))

    def s_sound_from_to_and_wait(self, th, b):
        return self._wait(max(0, to_num(self.p(th, b, 2)) - to_num(self.p(th, b, 1))))

    def s_read_text_wait_with_block(self, th, b):
        return self._wait(0)

    def e_get_pictures(self, th, b):
        return self.p(th, b, 0)

    e_get_sounds = e_get_pictures

    # ================= literals / operators =================
    def e_number(self, th, b):
        v = b["params"][0]
        n = parse_number(v)
        return n if n is not None else v

    def e_text(self, th, b):
        return b["params"][0]

    def e_angle(self, th, b):
        return self.e_number(th, b)

    def e_True(self, th, b):
        return True

    def e_get_boolean_value(self, th, b):
        return truthy(self.p(th, b, 0))

    def e_boolean_not(self, th, b):
        return not truthy(self.p(th, b, 1))

    def e_boolean_and_or(self, th, b):
        l = truthy(self.p(th, b, 0))
        op = self.p(th, b, 1)
        if op == "AND":
            return l and truthy(self.p(th, b, 2))
        return l or truthy(self.p(th, b, 2))

    def e_boolean_basic_operator(self, th, b):
        return compare(self.p(th, b, 0), self.p(th, b, 1), self.p(th, b, 2))

    def e_calc_basic(self, th, b):
        a, op, c = self.p(th, b, 0), self.p(th, b, 1), self.p(th, b, 2)
        if op == "PLUS":
            x, y = parse_number(a), parse_number(c)
            if x is not None and y is not None:
                return norm(x + y)
            return to_str(a) + to_str(c)
        x, y = to_num(a), to_num(c)
        if op == "MINUS":
            return norm(x - y)
        if op == "MULTI":
            return norm(x * y)
        if op == "DIVIDE":
            if y == 0:
                return math.nan if x == 0 or x != x else (math.inf if x > 0 else -math.inf)
            return norm(x / y)
        raise self._err("UNSUPPORTED_BLOCK", "calc_basic 연산 '%s' 는 지원하지 않습니다." % op, th)

    def e_calc_rand(self, th, b):
        a, c = to_num(self.p(th, b, 1)), to_num(self.p(th, b, 3))
        lo, hi = min(a, c), max(a, c)
        if isinstance(a, int) and isinstance(c, int):
            return self.rng.randint(lo, hi)
        return norm(round(self.rng.uniform(lo, hi), 2))

    def e_quotient_and_mod(self, th, b):
        x, y, op = to_num(self.p(th, b, 1)), to_num(self.p(th, b, 3)), self.p(th, b, 5)
        if y == 0:
            return math.nan
        if op == "QUOTIENT":
            return norm(math.floor(x / y))
        return norm(math.fmod(x, y))

    def e_calc_operation(self, th, b):
        v, op = to_num(self.p(th, b, 1)), self.p(th, b, 3)
        try:
            if op == "sin":
                return norm(round(math.sin(math.radians(v)), 12))
            if op == "cos":
                return norm(round(math.cos(math.radians(v)), 12))
            if op == "tan":
                return norm(round(math.tan(math.radians(v)), 12))
            if op == "asin_radian":
                return norm(math.degrees(math.asin(v)))
            if op == "acos_radian":
                return norm(math.degrees(math.acos(v)))
            if op == "atan_radian":
                return norm(math.degrees(math.atan(v)))
            if op == "ln":
                return norm(math.log(v)) if v > 0 else (-math.inf if v == 0 else math.nan)
            if op == "log":
                return norm(math.log10(v)) if v > 0 else (-math.inf if v == 0 else math.nan)
            if op == "floor":
                return math.floor(v)
            if op == "ceil":
                return math.ceil(v)
            if op == "round":
                return js_round(v)
            if op == "abs":
                return abs(v)
            if op == "factorial":
                return math.factorial(int(v)) if v >= 0 and v == int(v) and v < 171 else (math.inf if v >= 171 else math.nan)
            if op == "square":
                return norm(v * v)
            if op == "root":
                return norm(math.sqrt(v)) if v >= 0 else math.nan
        except (ValueError, OverflowError):
            return math.nan
        raise self._err("UNSUPPORTED_BLOCK", "calc_operation 연산 '%s' 는 지원하지 않습니다." % op, th)

    # strings
    def e_combine_something(self, th, b):
        return to_str(self.p(th, b, 1)) + to_str(self.p(th, b, 3))

    def e_length_of_string(self, th, b):
        return len(to_str(self.p(th, b, 1)))

    def e_reverse_of_string(self, th, b):
        return to_str(self.p(th, b, 1))[::-1]

    def e_char_at(self, th, b):
        s, i = to_str(self.p(th, b, 1)), self.p(th, b, 3)
        n = parse_number(i)
        if n is None or not 1 <= n <= len(s):
            raise self._err("INDEX_OUT_OF_RANGE", "글자 번호 %s 가 문자열 길이(%d)를 벗어났습니다." % (i, len(s)), th)
        return s[int(n) - 1]

    def e_substring(self, th, b):
        s, f, t = to_str(self.p(th, b, 1)), parse_number(self.p(th, b, 3)), parse_number(self.p(th, b, 5))
        if f is None or t is None or not (1 <= f <= len(s) and 1 <= t <= len(s)):
            raise self._err("INDEX_OUT_OF_RANGE", "문자열 범위 %s~%s 가 길이(%d)를 벗어났습니다." % (self.p(th, b, 3), self.p(th, b, 5), len(s)), th)
        lo, hi = int(min(f, t)), int(max(f, t))
        return s[lo - 1:hi]

    def e_index_of_string(self, th, b):
        return to_str(self.p(th, b, 1)).find(to_str(self.p(th, b, 3))) + 1

    def e_count_match_string(self, th, b):
        s, pat = to_str(self.p(th, b, 0)), to_str(self.p(th, b, 2))
        return s.count(pat) if pat else 0

    def e_replace_string(self, th, b):
        return to_str(self.p(th, b, 1)).replace(to_str(self.p(th, b, 3)), to_str(self.p(th, b, 5)))

    def e_change_string_case(self, th, b):
        s = to_str(self.p(th, b, 1))
        return s.upper() if self.p(th, b, 3) == "toUpperCase" else s.lower()

    def e_is_type(self, th, b):
        v, k = self.p(th, b, 0), self.p(th, b, 2)
        s = to_str(v)
        if k == "number":
            return parse_number(v) is not None
        if k == "ko":
            return bool(HANGUL.match(s))
        return bool(ENGLISH.match(s))

    def e_change_hex_to_rgb(self, th, b):
        h = to_str(self.p(th, b, 0)).lstrip("#")
        if len(h) == 3:
            h = "".join(c * 2 for c in h)
        try:
            r, g, bl = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
        except ValueError:
            raise self._err("BAD_COLOR", "색상 값이 잘못되었습니다: %r" % h, th)
        return {"r": r, "g": g, "b": bl}[self.p(th, b, 1)]

    def e_change_rgb_to_hex(self, th, b):
        c = [max(0, min(255, int(to_num(self.p(th, b, i))))) for i in range(3)]
        return "#%02x%02x%02x" % tuple(c)

    def e_get_date(self, th, b):
        d = self.now or datetime.datetime.now()
        u = self.p(th, b, 1)
        return {"YEAR": d.year, "MONTH": d.month, "DAY": d.day, "HOUR": d.hour, "MINUTE": d.minute,
                "SECOND": d.second, "DAY_OF_WEEK": (d.weekday() + 1) % 7 + 1}[u]

    def e_get_block_count(self, th, b):
        return self.block_total

    # ================= object info / collisions =================
    def e_coordinate_object(self, th, b):
        t, prop = self.p(th, b, 1), self.p(th, b, 3)
        objs = self._target_objs(th, t)
        if not objs:
            raise self._err("UNKNOWN_OBJECT", "대상 오브젝트 '%s' 를 찾을 수 없습니다." % t, th)
        o = objs[0]
        if prop == "x":
            return norm(o.x)
        if prop == "y":
            return norm(o.y)
        if prop == "rotation":
            return norm(o.rotation)
        if prop == "direction":
            return norm(o.direction)
        if prop == "size":
            return norm(round(o.size, 2))
        if prop == "picture_index":
            return o.pic + 1
        if prop == "picture_name":
            return o.pictures[o.pic].get("name", "") if o.pictures else ""
        raise self._err("UNSUPPORTED_BLOCK", "coordinate_object 속성 '%s' 는 지원하지 않습니다." % prop, th)

    def e_distance_something(self, th, b):
        tx, ty = self._target_pos(th, self.p(th, b, 1))
        return norm(math.hypot(tx - th.obj.x, ty - th.obj.y))

    def e_reach_something(self, th, b):
        t, o = self.p(th, b, 1), th.obj
        pts = o.corners()
        if t == "mouse":
            from .sim import point_in
            return point_in(pts, self.mouse["x"], self.mouse["y"])
        xs, ys = [c[0] for c in pts], [c[1] for c in pts]
        walls = {"wall": max(ys) >= 135 or min(ys) <= -135 or max(xs) >= 240 or min(xs) <= -240,
                 "wall_up": max(ys) >= 135, "wall_down": min(ys) <= -135,
                 "wall_right": max(xs) >= 240, "wall_left": min(xs) <= -240}
        if t in walls:
            return walls[t]
        from .sim import poly_hit
        for other in self.find_targets(t):
            if other is not o and other.visible and poly_hit(pts, other.corners()):
                return True
        return False

    # ---- misc value blocks ----
    def e_continue_repeat(self, th, b):
        # Tess `skip` compiles to wait_until_true(not continue_repeat): never blocks here.
        return False

    def e_get_translated_string(self, th, b):
        self._warn("STUBBED_BLOCK", "번역 블록은 시뮬레이터에서 원문을 그대로 돌려줍니다.")
        return to_str(self.p(th, b, 1))

    def _stub_text(self, th, b):
        self._warn("STUBBED_BLOCK", "네트워크/외부 의존 블록 '%s' 은 시뮬레이터에서 빈 값을 냅니다." % b["type"])
        return ""

    e_get_cur_weather = e_get_cur_wind = e_get_today_temperature = e_get_weather_data = _stub_text
    e_get_disaster_alert = e_get_disaster_guideline = e_get_festival_info = _stub_text
    e_get_korea_area_code = _stub_text
    e_check_weather = e_check_finedust = e_check_disaster_alert = lambda self, th, b: False
    e_count_disaster_guideline = e_count_festival = lambda self, th, b: 0
