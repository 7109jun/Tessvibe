"""Entry value semantics: loosely typed numbers/strings/booleans."""
import math
import re

_NUMERIC = re.compile(r"^\s*[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?\s*$")


def norm(n):
    """Collapses integral floats to int so dumps stay readable."""
    if isinstance(n, float) and n == n and n not in (math.inf, -math.inf) and n.is_integer() and abs(n) < 1e15:
        return int(n)
    return n


def parse_number(v):
    """Returns a number for numeric values/strings, otherwise None."""
    if isinstance(v, bool):
        return None
    if isinstance(v, (int, float)):
        return v
    if isinstance(v, str) and _NUMERIC.match(v):
        f = float(v)
        return norm(f)
    return None


def to_num(v, default=0):
    n = parse_number(v)
    if n is not None:
        return n
    if isinstance(v, bool):
        return 1 if v else 0
    return default


def fmt_number(n):
    if isinstance(n, int):
        return str(n)
    if n != n:
        return "NaN"
    if n == math.inf:
        return "Infinity"
    if n == -math.inf:
        return "-Infinity"
    if n.is_integer() and abs(n) < 1e21:
        return str(int(n))
    text = repr(n)
    return text.replace("e-0", "e-").replace("e+0", "e+")


def to_str(v):
    if v is None:
        return ""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return fmt_number(v)
    return str(v)


def truthy(v):
    if isinstance(v, bool):
        return v
    if v is None:
        return False
    if isinstance(v, (int, float)):
        return v != 0
    return str(v) not in ("", "0", "false", "FALSE")


def compare(a, op, b):
    na, nb = parse_number(a), parse_number(b)
    if na is not None and nb is not None:
        x, y = na, nb
    else:
        x, y = to_str(a), to_str(b)
    if op == "EQUAL":
        return x == y
    if op == "NOT_EQUAL":
        return x != y
    if op == "GREATER":
        return x > y
    if op == "LESS":
        return x < y
    if op == "GREATER_OR_EQUAL":
        return x >= y
    if op == "LESS_OR_EQUAL":
        return x <= y
    raise ValueError(op)


def js_round(x):
    return math.floor(x + 0.5)


def out_value(v):
    """Value as written to JSON/CSV dumps."""
    if isinstance(v, float):
        if v != v or v in (math.inf, -math.inf):
            return fmt_number(v)
        return norm(v)
    return v
