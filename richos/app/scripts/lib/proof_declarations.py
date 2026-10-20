"""Read script-suite dependencies separately from explicit behavioral coverage.

A suite that checks reviewed reader pins may also declare `# run-tests: pins <file>...`:
every reader such a file binds, and the file with its review, then selects the suite. The
list is read out of the pin file on every run, never typed into the suite, so a reader
added to a qualification selects its check the day it is added. Without this, a change to
a pinned reader selected nothing that checks the pin, and the first thing to notice was
the nightly (2026-09-28: proof-evidence.test.py refused 27 recipes at the script-suites
gate after 5b85f4fb changed four pinned readers at land).
"""
import json
from pathlib import Path, PurePosixPath
import re
import sys


class InvalidDeclaration(ValueError):
    pass


def _literal(path, where):
    if (not isinstance(path, str) or not re.fullmatch(r"[A-Za-z0-9_./-]+", path)
            or PurePosixPath(path).is_absolute()
            or any(part in {"", ".", ".."} for part in path.split("/"))):
        raise InvalidDeclaration(f"{where}: expected a literal repository-relative path, got {path!r}")
    return path


def pinned_readers(root, pin_file, where="pins"):
    """The pin file, its review, and every reader its units bind: the paths whose change
    must select the suite that checks the pins. A file that cannot be read as a pin file
    refuses rather than selecting nothing."""
    _literal(pin_file, where)
    path = Path(root) / pin_file
    if not path.is_file():
        raise InvalidDeclaration(f"{where}: pin file {pin_file} is not in the tree")
    try:
        document = json.loads(path.read_text())
    except ValueError as exc:
        raise InvalidDeclaration(f"{where}: pin file {pin_file} is not JSON: {exc}") from None
    units = document.get("units") if isinstance(document, dict) else None
    if not isinstance(units, dict) or not units:
        raise InvalidDeclaration(f"{where}: pin file {pin_file} binds no units")
    paths = {pin_file}
    if document.get("review") is not None:
        paths.add(_literal(document["review"], f"{where}: {pin_file} review"))
    for name, unit in units.items():
        sources = unit.get("sources") if isinstance(unit, dict) else None
        if not isinstance(sources, dict) or not sources:
            raise InvalidDeclaration(f"{where}: {pin_file} unit {name!r} pins no sources")
        for source in sources:
            paths.add(_literal(source, f"{where}: {pin_file} unit {name!r}"))
    return sorted(paths)


def read_declarations(root, directory):
    rows = []
    suites = sorted(directory.glob("*.test.sh"))
    if not suites:
        raise InvalidDeclaration(f"{directory}: no script suites")
    for suite in suites:
        declarations = {"inputs": [], "covers": [], "pins": [], "select-inputs": []}
        for number, line in enumerate(suite.read_text().splitlines(), 1):
            match = re.fullmatch(r"# run-tests: (inputs|covers|pins|select-inputs)(?:\s+(.*))?", line)
            if match:
                declarations[match[1]].append((number, (match[2] or "").split()))
        pins = declarations.pop("pins")
        selection = declarations.pop("select-inputs")
        if len(selection) > 1 or (selection and not selection[0][1]):
            raise InvalidDeclaration(f"{suite}: select-inputs must name runtime dependencies exactly once")
        for kind, claims in declarations.items():
            if len(claims) != 1:
                raise InvalidDeclaration(f"{suite}: expected exactly one '# run-tests: {kind}' row")
            number, paths = claims[0]
            if not paths or ("-" in paths and (kind != "covers" or paths != ["-"])):
                raise InvalidDeclaration(f"{suite}:{number}: {kind} must list paths; use 'covers -' for no coverage")
        if len(pins) > 1 or (pins and not pins[0][1]):
            raise InvalidDeclaration(f"{suite}: at most one '# run-tests: pins' row, and it names a pin file")
        inputs = list(declarations["inputs"][0][1])
        for number, files in pins:
            for pin_file in files:
                for path in pinned_readers(root, pin_file, f"{suite}:{number}"):
                    if path not in inputs:
                        inputs.append(path)
        number, covers = declarations["covers"][0]
        if covers == ["-"]:
            covers = []
        for path in covers:
            where = f"{suite}:{number}: covers {path}"
            if (not re.fullmatch(r"[A-Za-z0-9_./-]+", path)
                    or PurePosixPath(path).is_absolute()
                    or any(part in {"", ".", ".."} for part in path.split("/"))):
                raise InvalidDeclaration(f"{where}: expected a literal repository-relative path")
            if (root / path).is_dir():
                raise InvalidDeclaration(f"{where}: directory coverage would hide an orphan; list tested files instead")
            if not (root / path).is_file():
                raise InvalidDeclaration(f"{where}: path is not in the tree")
            if not any(path == dep or path.startswith(dep + "/") for dep in inputs):
                raise InvalidDeclaration(f"{where}: not selected by this suite's inputs")
        selected = selection[0][1] if selection else inputs
        for dep in (selected if selection else []):
            _literal(dep, f"{suite}: select-inputs")
            if not any(dep == broad or dep.startswith(broad + "/") for broad in inputs):
                raise InvalidDeclaration(f"{suite}: selection input {dep} is not an evidence input")
            if not (root / dep).exists():
                raise InvalidDeclaration(f"{suite}: selection input {dep} is missing")
        if selection and any(not any(p == dep or p.startswith(dep + "/") for dep in selected) for p in covers):
            raise InvalidDeclaration(f"{suite}: runtime selection omits a covered dependency")
        rows.append((suite.name, selected, covers))
    return rows


def suite_pins(root, suite):
    """The pin-derived paths of one suite (run-tests.sh's skip digest reads these)."""
    out = []
    for number, line in enumerate(Path(suite).read_text().splitlines(), 1):
        match = re.fullmatch(r"# run-tests: pins(?:\s+(.*))?", line)
        if match:
            files = (match[1] or "").split()
            if out or not files:
                raise InvalidDeclaration(f"{suite}: at most one '# run-tests: pins' row, and it names a pin file")
            for pin_file in files:
                out.extend(p for p in pinned_readers(root, pin_file, f"{suite}:{number}") if p not in out)
    return out


def suite_gate_inputs(root, suite, inputs):
    """`# run-tests: gate-inputs <dir>...`: directory inputs the suite checks file by file, so a
    change to any file under them selects it at the merge gate too (proof-for.sh --gate), where
    a directory input otherwise selects only for a product path. Each must be a directory in the
    tree and among the suite's inputs. Without it, the land of 0fe37b286 (2026-10-20) added a UI
    test reading a new environment name, never ran proof-qualification, and nightly 69 failed it."""
    rows = []
    for number, line in enumerate(Path(suite).read_text().splitlines(), 1):
        match = re.fullmatch(r"# run-tests: gate-inputs(?:\s+(.*))?", line)
        if match:
            rows.append((number, (match[1] or "").split()))
    if len(rows) > 1 or (rows and not rows[0][1]):
        raise InvalidDeclaration(f"{suite}: at most one '# run-tests: gate-inputs' row, and it names directories")
    out = []
    for number, paths in rows:
        for path in paths:
            where = f"{suite}:{number}: gate-inputs {path}"
            _literal(path, where)
            if not (Path(root) / path).is_dir():
                raise InvalidDeclaration(f"{where}: not a directory in the tree")
            if not any(path == dep or path.startswith(dep + "/") for dep in inputs):
                raise InvalidDeclaration(f"{where}: not among this suite's inputs")
            out.append(path)
    return out


def main():
    if sys.argv[1:2] == ["--pinned"] and len(sys.argv) == 4:
        try:
            print("\n".join(suite_pins(Path(sys.argv[2]), Path(sys.argv[3]))))
        except (OSError, InvalidDeclaration) as exc:
            print(f"run-tests.sh: {exc}", file=sys.stderr)
            return 2
        return 0
    try:
        root, directory = Path(sys.argv[1]), Path(sys.argv[2])
        rows = [(suite, inputs, covers, suite_gate_inputs(root, directory / suite, inputs))
                for suite, inputs, covers in read_declarations(root, directory)]
    except (OSError, InvalidDeclaration) as exc:
        print(f"proof-for.sh: {exc}", file=sys.stderr)
        return 2
    for suite, inputs, covers, gate in rows:
        print("inputs", suite, " ".join(inputs), sep="\t")
        for path in covers:
            print("covers", suite, path, sep="\t")
        for path in gate:
            print("gate", suite, path, sep="\t")
    return 0


if __name__ == "__main__":
    sys.exit(main())
