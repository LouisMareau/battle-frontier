"""Floor tiles laid by wave function collapse, by the engine's rules (crates/world/src/floors.rs),
in plain Python: for the previews of a set of floor tiles a request makes (request_kit.layout)
and their tests.

A tile is (name, edges, weight, turns): its four edges' words going round from the north (the
model's +Y in Blender, -z in the engine), then east (+X), south and west, as it's modelled; how
often it's picked among those that fit; whether it may be laid turned (a quarter, a half,
three quarters, clockwise seen from above). Two tiles side by side fit when the edge between
them is the same word on both. Past the cells laid only what may end there faces out: rims
when a tile has an edge "rim" (and they face nothing else), else the words a tile has on all
four edges, else anything. The cell with the fewest ways left is decided first, by weight;
on a dead end it starts again from the next seed; past eight tries, what fits best. A tile put
by hand may lie mirrored (its east and west swapped, then turned): (name, turn, True) among
the cells laid already; the laying itself never mirrors.
"""
import random

SIDES = [(0, -1), (1, 0), (0, 1), (-1, 0)]
RIM = "rim"


def edge(edges, turn, side, flip=False):
    """A tile's edge on a side (0 north, 1 east, 2 south, 3 west) laid `turn` quarter turns
    clockwise: what was on the side `turn` steps back round; mirrored first when `flip` (its
    east and west swapped)."""
    s = (side + 4 - turn % 4) % 4
    return edges[(4 - s) % 4 if flip else s]


def ways(tiles):
    """Every way the tiles lie: (name, turn, its edges as it lies, weight), each tile's weight
    shared among its turns."""
    out = []
    for name, edges, weight, turns in tiles:
        n = 4 if turns else 1
        for t in range(n):
            out.append((name, t, [edge(edges, t, s) for s in range(4)], weight / n))
    return out


def lay(tiles, cells, seed=1, fixed=None):
    """`cells` ((x, z) pairs) laid with the tiles: {cell: (name, turn)}. `fixed`: cells laid
    already {cell: (name, turn) or (name, turn, mirrored)} of these tiles, binding those next
    to them."""
    fixed = fixed or {}
    ws = ways(tiles)
    by_name = {t[0]: t for t in tiles}
    cells = sorted(set(cells))
    if not ws or not cells:
        return {}
    index = {c: i for i, c in enumerate(cells)}
    rims = any(RIM in w[2] for w in ws)
    fillers = sorted({w[2][0] for w in ws if len(set(w[2])) == 1})
    outside = [RIM] if rims else fillers or None
    # what binds each side of each cell: ("cell", j), ("words", [...]) or None
    binds = []
    for (x, z) in cells:
        row = []
        for s, (dx, dz) in enumerate(SIDES):
            n = (x + dx, z + dz)
            if n in index:
                row.append(("cell", index[n]))
            elif n in fixed and fixed[n][0] in by_name:
                name, turn, *mirrored = fixed[n]
                row.append(("words", [edge(by_name[name][1], turn, (s + 2) % 4, bool(mirrored and mirrored[0]))]))
            else:
                row.append(("words", outside) if outside else None)
        binds.append(row)

    def fits(b, w):
        for s in range(4):
            if b[s] is None:
                continue
            kind, v = b[s]
            if kind == "words" and w[2][s] not in v:
                return False
            if kind == "cell" and rims and w[2][s] == RIM:
                return False
        return True

    start = []
    for b in binds:
        alive = [fits(b, w) for w in ws]
        start.append(alive if any(alive) else [True] * len(ws))

    def settle(alive, changed):
        while changed:
            i = changed.pop()
            for s in range(4):
                if binds[i][s] is None or binds[i][s][0] != "cell":
                    continue
                j = binds[i][s][1]
                words = {ws[w][2][s] for w in range(len(ws)) if alive[i][w]}
                removed = False
                for w in range(len(ws)):
                    if alive[j][w] and ws[w][2][(s + 2) % 4] not in words:
                        alive[j][w] = False
                        removed = True
                if removed:
                    if not any(alive[j]):
                        return False
                    if j not in changed:
                        changed.append(j)
        return True

    for attempt in range(8):
        dice = random.Random(seed * 1000003 + attempt)
        alive = [list(a) for a in start]
        if not settle(alive, list(range(len(cells)))):
            continue
        stuck = False
        while True:
            open_cells = [i for i in range(len(cells)) if sum(alive[i]) > 1]
            if not open_cells:
                break
            i = min(open_cells, key=lambda k: (sum(alive[k]), k))
            choices = [w for w in range(len(ws)) if alive[i][w]]
            r = dice.random() * sum(ws[w][3] for w in choices)
            pick = choices[-1]
            for w in choices:
                r -= ws[w][3]
                if r < 0:
                    pick = w
                    break
            alive[i] = [w == pick for w in range(len(ws))]
            if not settle(alive, [i]):
                stuck = True
                break
        if not stuck:
            return {c: ws[a.index(True)][:2] for c, a in zip(cells, alive)}
    # past the tries: what agrees most with the cells decided before, the likeliest
    chosen = [None] * len(cells)
    for i in range(len(cells)):
        def agrees(w):
            n = 0
            for s in range(4):
                b = binds[i][s]
                if b is None:
                    continue
                if b[0] == "cell" and chosen[b[1]] is not None and ws[chosen[b[1]]][2][(s + 2) % 4] == ws[w][2][s]:
                    n += 1
                elif b[0] == "words" and ws[w][2][s] in b[1]:
                    n += 1
            return (n, ws[w][3])
        chosen[i] = max((w for w in range(len(ws)) if start[i][w]), key=agrees, default=0)
    return {c: ws[w][:2] for c, w in zip(cells, chosen)}


def agree(tiles, laid):
    """Where two cells side by side don't agree, and what faces out that may not: a list of
    what's wrong (none: every pair fits)."""
    by_name = {t[0]: t for t in tiles}
    ws = ways(tiles)
    rims = any(RIM in w[2] for w in ws)
    fillers = {w[2][0] for w in ws if len(set(w[2])) == 1}
    out = []
    for (x, z), (name, turn, *mirrored) in laid.items():
        for s, (dx, dz) in enumerate(SIDES):
            mine = edge(by_name[name][1], turn, s, bool(mirrored and mirrored[0]))
            n = laid.get((x + dx, z + dz))
            if n is not None:
                theirs = edge(by_name[n[0]][1], n[1], (s + 2) % 4, bool(len(n) > 2 and n[2]))
                if mine != theirs:
                    out.append(f"{x},{z} {name}/{turn} meets {x + dx},{z + dz} {n[0]}/{n[1]}: {mine} against {theirs}")
            elif rims and mine != RIM:
                out.append(f"{x},{z} {name}/{turn}: {mine} facing out, not a rim")
            elif not rims and fillers and mine not in fillers:
                out.append(f"{x},{z} {name}/{turn}: {mine} facing out")
    return out
