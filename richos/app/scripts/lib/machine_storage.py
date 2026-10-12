"""App-tool transport to the engine's one Python machine-settings reader."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'engine/scripts/lib'))
from machine_settings import KEYS, RESOLVED, load, environment, validate, cache, state, agent_root
