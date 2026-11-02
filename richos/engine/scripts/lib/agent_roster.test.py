#!/usr/bin/env python3
"""agent_roster.py: a lease's own roster first, the entity's folder where there is none (terminal)."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("agent_roster", HERE / "agent_roster.py")
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)


class Roster(unittest.TestCase):
    def test_the_leases_roster_wins_over_the_entity_folder(self):
        with patch.dict(os.environ, {R.ENV: "/lease/plugin/agents"}):
            self.assertEqual(R.roster_dir("/entity"), Path("/lease/plugin/agents"))

    def test_the_entity_folder_stands_where_the_variable_is_unset_or_empty(self):
        for value in (None, ""):
            env = {k: v for k, v in os.environ.items() if k != R.ENV}
            if value is not None:
                env[R.ENV] = value
            with patch.dict(os.environ, env, clear=True):
                self.assertEqual(R.roster_dir("/entity"), Path("/entity/.claude/agents"))


if __name__ == "__main__":
    unittest.main()
