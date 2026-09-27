"""A simple bot that plays Neon Swarm through forge commands: clicks START, then keeps aiming
at the nearest enemy and firing, strafing to dodge. Prints what happened.

    python examples/bot_neon_swarm.py [path/to/forge.exe] [world]
"""
import json
import subprocess
import sys

import os
exe = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join("target", "release", "forge.exe"))
world = sys.argv[2] if len(sys.argv) > 2 else "worlds/neon-swarm"
p = subprocess.Popen([exe, "--no-autosave", world], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8")


def cmd(c):
    p.stdin.write(json.dumps(c) + "\n")
    p.stdin.flush()
    return json.loads(p.stdout.readline())


cmd({"cmd": "step", "ticks": 2})
cmd({"cmd": "input", "ui_click": "start"})
cmd({"cmd": "step", "ticks": 5})
counts = {}
for loop in range(90):
    me = cmd({"cmd": "query", "kind": "ship"})["results"]
    if not me:
        break
    me = me[0]
    enemies = cmd({"cmd": "query", "where": 'e.tags != () && e.tags.contains("enemy")'}).get("results", [])
    if enemies:
        t = min(enemies, key=lambda e: (e["x"] - me["x"]) ** 2 + (e["y"] - me["y"]) ** 2)
        cmd({"cmd": "input", "mouse": [t["x"] + t.get("w", 1) / 2, t["y"] + t.get("h", 1) / 2], "mouse_down": "left"})
    else:
        cmd({"cmd": "input", "mouse_up": "left"})
    # strafe: circle around the middle of the arena
    keys = [["up"], ["right"], ["down"], ["left"]][(loop // 6) % 4]
    r = cmd({"cmd": "step", "ticks": 10, "inputs": {"0": keys}})
    for k, v in r["events"]["counts"].items():
        counts[k] = counts.get(k, 0) + v
    if r.get("screen") and r["screen"]["title"] == "GAME OVER":
        break

look = cmd({"cmd": "look"})
print("ticks:", look["tick"], "| wave:", look["vars"].get("wave"), "| score:", look["game"].get("score"),
      "| over:", look["vars"].get("over", False))
ship = cmd({"cmd": "query", "kind": "ship"})["results"]
print("ship hp:", ship[0]["hp"] if ship else "gone")
print("events:", {k: counts[k] for k in sorted(counts)})
print("ui:", sorted(look["ui"].keys()))
shot = cmd({"cmd": "screenshot", "save": os.path.join(os.environ.get("TEMP", "."), "neon-swarm-bot.png"), "data": False})
print("screenshot:", shot.get("saved"), shot.get("width"), "x", shot.get("height"))
p.stdin.close()
p.wait()
