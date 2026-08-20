#!/usr/bin/env bash

link_points_into_dir() {
  local link_path="$1"
  local root="$2"
  local target

  [[ -n "${root}" ]] || return 1
  [[ -L "${link_path}" ]] || return 1
  target="$(readlink "${link_path}")" || return 1
  case "${target}" in
    "${root}"/*) return 0 ;;
    *) return 1 ;;
  esac
}

remove_link_if_points_into() {
  local link_path="$1"
  local root="$2"
  local link_dir

  link_points_into_dir "${link_path}" "${root}" || return 0
  link_dir="$(dirname "${link_path}")"
  if [[ -w "${link_dir}" ]]; then
    rm -f "${link_path}"
  else
    sudo rm -f "${link_path}"
  fi
}

ensure_link_available() {
  local link_path="$1"
  local managed_target="$2"

  if [[ -z "${managed_target}" ]]; then
    echo "error: refusing to manage a command link without a target" >&2
    return 1
  fi
  if [[ ! -e "${link_path}" && ! -L "${link_path}" ]]; then
    return 0
  fi
  if [[ -L "${link_path}" && "$(readlink "${link_path}")" == "${managed_target}" ]]; then
    return 0
  fi
  echo "error: refusing to replace existing command path: ${link_path}" >&2
  return 1
}

install_managed_link() {
  local link_path="$1"
  local managed_target="$2"
  local link_dir

  ensure_link_available "${link_path}" "${managed_target}" || return 1
  if [[ -L "${link_path}" && "$(readlink "${link_path}")" == "${managed_target}" ]]; then
    return 0
  fi

  link_dir="$(dirname "${link_path}")"
  if [[ -d "${link_dir}" && -w "${link_dir}" ]]; then
    ln -s "${managed_target}" "${link_path}"
  else
    sudo mkdir -p "${link_dir}"
    sudo ln -s "${managed_target}" "${link_path}"
  fi
}

validate_cleanup_inputs() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"

  if [[ -z "${user_home}" || "${user_home}" == "/" || "${user_home}" != /* ]]; then
    echo "error: refusing legacy cleanup for unsafe HOME: ${user_home}" >&2
    return 1
  fi
  if [[ -z "${link_dir}" || "${link_dir}" == "/" || "${link_dir}" != /* ]]; then
    echo "error: refusing legacy cleanup for unsafe link directory: ${link_dir}" >&2
    return 1
  fi
  if [[ ! "${user_uid}" =~ ^[0-9]+$ ]]; then
    echo "error: refusing legacy cleanup for invalid user id: ${user_uid}" >&2
    return 1
  fi
}

cleanup_legacy_install() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"
  local support_root
  local legacy_root
  local legacy_plist
  local legacy_label

  validate_cleanup_inputs "${user_home}" "${link_dir}" "${user_uid}" || return 1

  user_home="${user_home%/}"
  link_dir="${link_dir%/}"
  support_root="${user_home}/Library/Application Support"
  legacy_root="${support_root}/QuickSync"
  legacy_plist="${user_home}/Library/LaunchAgents/com.quicksync.qsd.plist"
  legacy_label="gui/${user_uid}/com.quicksync.qsd"

  [[ "${legacy_root}" == "${support_root}/QuickSync" ]] || return 1

  if command -v launchctl >/dev/null 2>&1; then
    launchctl bootout "${legacy_label}" >/dev/null 2>&1 || true
    if [[ -e "${legacy_plist}" ]]; then
      launchctl bootout "gui/${user_uid}" "${legacy_plist}" >/dev/null 2>&1 || true
    fi
  fi
  rm -f "${legacy_plist}"

  remove_link_if_points_into "${link_dir}/qs" "${legacy_root}/bin"
  remove_link_if_points_into "${link_dir}/qsd" "${legacy_root}/bin"

  if [[ -e "${legacy_root}" || -L "${legacy_root}" ]]; then
    rm -rf "${legacy_root}"
  fi
}
