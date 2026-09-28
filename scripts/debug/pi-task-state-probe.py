"""Diagnostic probe (debug-only): is the Pi RPC process alive and registered?

Read-only check of task state in the local DeepPi database, used while
investigating "clicked a task but nothing happened" bugs. Not part of the app.
"""
import sqlite3, os, datetime

db = os.path.expandvars(r"%APPDATA%\com.deeppi.desktop\deeppi.db")
con = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=8)
con.row_factory = sqlite3.Row


def ts(v):
    if not v:
        return "-"
    return datetime.datetime.fromtimestamp(v / 1000 if v > 1e12 else v).strftime("%m-%d %H:%M:%S")


print("=== pi tasks, newest first ===")
for r in con.execute("SELECT * FROM tasks WHERE agent='pi' ORDER BY created_at DESC LIMIT 6"):
    d = dict(r)
    print(f"  created={ts(d['created_at'])} started={ts(d['started_at'])} completed={ts(d['completed_at'])}")
    print(f"    status={d['status']:9} mode={d['interaction_mode']} id={d['id']}")
    print(f"    session_id={d['session_id']}")
    print(f"    session_file={d['session_file']}")

print("\n=== rows still claiming to be live ===")
for r in con.execute(
        "SELECT id, agent, status, created_at FROM tasks "
        "WHERE status IN ('queued','running','waiting') ORDER BY created_at DESC"):
    print(f"  {r['agent']:4} {r['status']:8} {ts(r['created_at'])}  {r['id']}")

con.close()
