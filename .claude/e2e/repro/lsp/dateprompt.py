"""The date prompt document (full client): calendar keys, typing, picking a
time, preview, cancel; the entry's file after each."""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from client import *
import workspace
ws, options = workspace.make(os.path.join(SCRATCH, "ws-dp"))
c = Client(profile=FULL, root=ws, options=options, name="dp")
g = c.open(os.path.join(ws, "gtd.org"))
c.settle(0.3, 5)
def planning(needle):
    lines = c.text(g).split("\n")
    i = next(i for i, l in enumerate(lines) if needle in l)
    return lines[i + 1].strip()
def date_doc():
    return [u for u in c.docs if "Date" in u][-1]
def sel():
    s = [x for x in c.shown if "Date" in x["uri"]][-1].get("selection")
    return s["start"] if s else None
def cal(cmd, **extra):
    u = date_doc(); p = sel()
    a = c.at(u, p["line"], p["character"], **extra)
    r = c.command(cmd, a); c.settle(0.4, 5); c.run_fetches()
    return r
# 1. Schedule, move the day with the calendar keys, select.
r = c.command("org.schedule", c.at_text(g, "Call the plumber")); c.settle(0.4, 5); c.run_fetches()
print("schedule ->", r, "shown", [s["uri"] for s in c.shown][-1:], "selection", sel())
print(c.docs[date_doc()]["text"])
for cmd in ("org.calendar.day.later", "org.calendar.week.later", "org.calendar.month.later", "org.calendar.month.earlier",
            "org.calendar.year.later", "org.calendar.year.earlier", "org.calendar.day.earlier", "org.calendar.scroll.later", "org.calendar.scroll.earlier"):
    r = cal(cmd)
    print(f"{cmd:32} -> {r} | sel {sel()} | last line: {c.docs[date_doc()]['text'].rstrip().splitlines()[-1]!r}")
# tokens of the date doc
toks = c.request("textDocument/semanticTokens/full", {"textDocument": {"uri": date_doc()}})
dec = decode_tokens(toks["data"], c.legend, c.docs[date_doc()]["text"], c.encoding)
print("date tokens:", sorted({(t[3], tuple(t[4])) for t in dec}))
print("selected:", [t for t in dec if t[3] == "calendarSelected"])
# preview while typing
pr = c.command("org.date.preview", dict(c.at(date_doc(), sel()["line"], sel()["character"]), command="org.schedule", input="fri 9am"))
print("preview fri 9am:", pr)
try:
    pr = c.command("org.date.preview", dict(c.at(date_doc(), sel()["line"], sel()["character"]), command="org.schedule", input="zzz"))
    print("preview zzz:", pr)
except ResponseError as e:
    print("preview zzz error:", e)
r = cal("org.calendar.select")
print("select ->", r, "| planning:", planning("Call the plumber"), "| date docs open:", [u for u in c.docs if "Date" in u])
if r and r.get("close"):
    c.close(r["close"])
# 2. Type an answer
r = c.command("org.deadline", c.at_text(g, "Weekly review")); c.settle(0.4, 5); c.run_fetches()
r1 = cal("org.calendar.type")
print("type ->", r1)
r2 = cal("org.calendar.type", input="+2w")
print("type +2w ->", r2, "| planning:", planning("Weekly review"))
if r2 and r2.get("close"):
    c.close(r2["close"])
# 3. Time picking
r = c.command("org.schedule", c.at_text(g, "Reply from Alice")); c.settle(0.4, 5); c.run_fetches()
for cmd in ("org.calendar.time", "org.calendar.day.later", "org.calendar.day.later", "org.calendar.week.later", "org.calendar.day.earlier", "org.calendar.time", "org.calendar.day.later"):
    r = cal(cmd)
    t = c.docs[date_doc()]["text"].rstrip().splitlines()
    print(f"{cmd:28} -> {r} | {t[-2]!r} | {t[-1]!r}")
r = cal("org.calendar.select")
print("select ->", r, "| planning:", planning("Reply from Alice"))
if r and r.get("close"):
    c.close(r["close"])
# 4. Cancel
r = c.command("org.schedule", c.at_text(g, "Groceries")); c.settle(0.4, 5); c.run_fetches()
r = cal("org.calendar.cancel")
print("cancel ->", r, "| planning:", planning("Groceries"), "| messages:", [m["message"] for m in c.messages][-2:])
if r and r.get("close"):
    c.close(r["close"])
# 5. Today
r = c.command("org.timestamp.insert", c.at_text(g, "A paragraph.", delta=12)); c.settle(0.4, 5); c.run_fetches()
cal("org.calendar.month.later")
r = cal("org.calendar.today")
print("today ->", r, sel(), c.docs[date_doc()]['text'].rstrip().splitlines()[-1])
r = cal("org.calendar.select")
print("select ->", r, [l for l in c.text(g).split("\n") if "A paragraph" in l])
print("edit failures", c.edit_failures, "stderr", c.stderr()[-500:])
c.shutdown()
