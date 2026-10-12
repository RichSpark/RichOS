"""Mobile-tool transport to the engine's one Python machine-settings reader."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'engine/scripts/lib'))
from machine_settings import load, environment, validate, cache, state, agent_root
