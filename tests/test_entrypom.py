import json
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)

from entrypom import EntrypomError, Scenario, Simulator, load_project  # noqa: E402
from entrypom.project import _find_tess_ent  # noqa: E402

NEEDS_TESS = unittest.skipUnless(_find_tess_ent(), "tess-ent 실행 파일이 없습니다")


def sim_of(source, scenario=None, **kw):
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "t.tess")
        with open(path, "w", encoding="utf-8") as f:
            f.write(source)
        project = load_project(path)
    return Simulator(project, Scenario(scenario), **kw)


@NEEDS_TESS
class RunTests(unittest.TestCase):
    def test_arithmetic_and_function(self):
        r = sim_of('''
var a = 0
function 두배(v):
  return v * 2
end
object "o":
  when start do
    a = 두배(3) + 1
  end
end
''').run()
        self.assertTrue(r["ok"])
        self.assertEqual(r["final"]["variables"]["a"], 7)

    def test_wait_uses_virtual_time(self):
        r = sim_of('''
var a = 0
object "o":
  when start do
    wait 1
    a = 1
  end
end
''').run()
        self.assertEqual(r["final"]["variables"]["a"], 1)
        self.assertAlmostEqual(r["time"], 1.0, delta=0.05)

    def test_signal_and_dump_trigger(self):
        r = sim_of('''
var a = 0
object "o":
  when start do
    a = 5
    send "dump"
    a = 9
  end
end
''').run()
        self.assertEqual(r["snapshots"][0]["trigger"], "signal:dump")
        self.assertEqual(r["snapshots"][0]["variables"]["a"], 5)
        self.assertEqual(r["final"]["variables"]["a"], 9)

    def test_var_trigger_and_lists(self):
        r = sim_of('''
var a = 0
list l = [1]
object "o":
  when start do
    in l add 2
    a = 1
  end
end
''', dump_on=["var=a", "list"]).run()
        self.assertEqual([s["trigger"] for s in r["snapshots"]], ["list", "var:a"])
        self.assertEqual(r["final"]["lists"]["l"], [1, 2])

    def test_ask_answer(self):
        r = sim_of('''
var n = ""
object "o":
  when start do
    ask "이름?"
    n = answer
  end
end
''', {"answers": ["민수"]}).run()
        self.assertEqual(r["final"]["variables"]["n"], "민수")

    def test_ask_without_answer_is_error(self):
        r = sim_of('''
object "o":
  when start do
    ask "?"
  end
end
''').run()
        self.assertFalse(r["ok"])
        self.assertEqual(r["error"]["code"], "NO_ANSWER")

    def test_key_press(self):
        sc = Scenario()
        sc.press("space", 0.2)
        s = sim_of('''
var k = 0
object "o":
  when key "space" do
    k = 1
  end
end
''')
        s.scenario = sc
        s.reset()
        r = s.run()
        self.assertEqual(r["final"]["variables"]["k"], 1)

    def test_list_index_error_is_json(self):
        r = sim_of('''
list l = [1]
object "o":
  when start do
    l[5] = 1
  end
end
''').run()
        self.assertFalse(r["ok"])
        self.assertEqual(r["error"]["code"], "INDEX_OUT_OF_RANGE")
        self.assertIn("block_id", r["error"])
        json.dumps(r)

    def test_clone_runs_cloned_script(self):
        r = sim_of('''
var n = 0
object "o":
  when start do
    clone
  end
  when cloned do
    n += 1
    kill
  end
end
''').run()
        self.assertEqual(r["final"]["variables"]["n"], 1)

    def test_table(self):
        r = sim_of('''
var s = 0
table t as "t":
  columns "a", "b"
  row "x", 5
end
object "o":
  when start do
    s = t[1, "b"]
    t[1, "b"] = 7
  end
end
''').run()
        self.assertEqual(str(r["final"]["variables"]["s"]), "5")
        self.assertEqual(r["final"]["tables"]["t"]["rows"], [["x", "7"]])

    def test_stop_all(self):
        r = sim_of('''
object "o":
  when start do
    forever:
      stop all
    end
  end
end
''').run()
        self.assertEqual(r["reason"], "stop")


@NEEDS_TESS
class CliTests(unittest.TestCase):
    def run_cli(self, *args):
        return subprocess.run([sys.executable, "-m", "entrypom", *args], cwd=ROOT, capture_output=True, text=True)

    def test_compile_error_is_json(self):
        with tempfile.NamedTemporaryFile("w", suffix=".tess", delete=False, encoding="utf-8") as f:
            f.write("object \"o\":\n  when start do\n    ???\n  end\nend\n")
        try:
            p = self.run_cli("run", f.name)
        finally:
            os.unlink(f.name)
        self.assertEqual(p.returncode, 2)
        doc = json.loads(p.stdout)
        self.assertFalse(doc["ok"])
        self.assertEqual(doc["error"]["code"], "COMPILE_ERROR")

    def test_csv(self):
        with tempfile.NamedTemporaryFile("w", suffix=".tess", delete=False, encoding="utf-8") as f:
            f.write('var a = 3\nlist l = [1, 2]\nobject "o":\n  when start do\n    a = 4\n  end\nend\n')
        try:
            p = self.run_cli("run", f.name, "--format", "csv")
        finally:
            os.unlink(f.name)
        self.assertEqual(p.returncode, 0)
        self.assertIn("final,", p.stdout.splitlines()[1])
        self.assertIn("global,variable,a,,4", p.stdout)
        self.assertIn("global,list,l,2,2", p.stdout)

    def test_missing_file(self):
        p = self.run_cli("run", "/nonexistent.ent")
        self.assertEqual(p.returncode, 2)
        self.assertEqual(json.loads(p.stdout)["error"]["code"], "FILE_NOT_FOUND")


class UnitTests(unittest.TestCase):
    def test_values(self):
        from entrypom.values import compare, to_str
        self.assertTrue(compare("10", "EQUAL", 10))
        self.assertTrue(compare(10, "GREATER", "9"))
        self.assertTrue(compare("abc", "LESS", "abd"))
        self.assertEqual(to_str(2.0), "2")

    def test_bad_scenario(self):
        with self.assertRaises(EntrypomError):
            Scenario({"events": [{"t": 1}]})


if __name__ == "__main__":
    unittest.main()
