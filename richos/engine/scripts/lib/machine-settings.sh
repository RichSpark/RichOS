# shellcheck shell=bash
# Source this helper once at a launcher. Python validates the file and quotes each
# literal value; this evaluates generated exports, never private file contents.
richos_machine_settings() {
  local settings_dir settings_exports
  settings_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)" || return
  settings_exports="$(python3 "$settings_dir/machine_settings.py" env --array)" || return
  eval "$settings_exports"
}
