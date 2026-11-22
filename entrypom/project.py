"""Loads .ent / project.json / .tess into a project dict."""
import json
import os
import shutil
import subprocess
import tarfile
import tempfile


class EntrypomError(Exception):
    """Any failure that is reported to the caller as JSON."""

    def __init__(self, code, message, **detail):
        super().__init__(message)
        self.code = code
        self.message = message
        self.detail = {k: v for k, v in detail.items() if v is not None}

    def to_dict(self):
        d = {"code": self.code, "message": self.message}
        d.update(self.detail)
        return d


def _find_tess_ent():
    env = os.environ.get("TESS_ENT")
    if env and os.path.exists(env):
        return env
    found = shutil.which("tess-ent")
    if found:
        return found
    here = os.path.dirname(os.path.abspath(__file__))
    bin_dir = os.path.join(os.path.dirname(here), "bin")
    import platform
    sysname = platform.system()
    machine = platform.machine().lower()
    names = []
    if sysname == "Windows":
        names = ["tess-ent-windows-x86_64.exe"]
    elif sysname == "Darwin":
        names = ["tess-ent-macos-arm64"]
    else:
        names = ["tess-ent-linux-x86_64"]
    for n in names:
        p = os.path.join(bin_dir, n)
        if os.path.exists(p):
            return p
    return None


def compile_tess(path):
    exe = _find_tess_ent()
    if not exe:
        raise EntrypomError("TESS_ENT_NOT_FOUND", "tess-ent 실행 파일을 찾지 못했습니다. PATH 에 두거나 TESS_ENT 환경변수를 지정하세요.")
    tmp = tempfile.mkdtemp(prefix="entrypom_")
    out = os.path.join(tmp, "project.json")
    try:
        r = subprocess.run([exe, "build", path, "-o", out, "--json"], capture_output=True, text=True)
        try:
            info = json.loads(r.stdout) if r.stdout.strip() else {}
        except ValueError:
            info = {}
        if r.returncode != 0 or not os.path.exists(out):
            raise EntrypomError("COMPILE_ERROR", "Tess 컴파일에 실패했습니다.",
                                errors=info.get("errors") or [{"message": (r.stderr or r.stdout).strip()}],
                                warnings=info.get("warnings"))
        with open(out, encoding="utf-8") as f:
            project = json.load(f)
        project["__compile"] = {"warnings": info.get("warnings", []), "notices": info.get("notices", [])}
        return project
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def load_project(path):
    if not os.path.exists(path):
        raise EntrypomError("FILE_NOT_FOUND", "파일이 없습니다: %s" % path, file=path)
    low = path.lower()
    try:
        if low.endswith(".tess"):
            return compile_tess(path)
        if low.endswith(".json"):
            with open(path, encoding="utf-8") as f:
                return json.load(f)
        with tarfile.open(path, "r:*") as tar:
            member = None
            for m in tar.getmembers():
                if m.name.replace("\\", "/").endswith("project.json"):
                    member = m
                    break
            if member is None:
                raise EntrypomError("BAD_ENT", ".ent 안에 project.json 이 없습니다.", file=path)
            return json.loads(tar.extractfile(member).read().decode("utf-8"))
    except EntrypomError:
        raise
    except (tarfile.TarError, ValueError, OSError) as e:
        raise EntrypomError("LOAD_ERROR", "작품을 읽지 못했습니다: %s" % e, file=path)
