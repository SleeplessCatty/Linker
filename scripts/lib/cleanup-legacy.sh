#!/usr/bin/env bash

legacy_cleanup_error() {
  echo "error: $*" >&2
}

trim_trailing_slash() {
  local path="$1"

  while [[ "${path}" != "/" && "${path}" == */ ]]; do
    path="${path%/}"
  done
  printf '%s\n' "${path}"
}

path_has_unsafe_alias() {
  local path="$1"
  local trimmed

  [[ -n "${path}" && "${path}" == /* ]] || return 0
  [[ "${path}" != *"//"* ]] || return 0
  [[ "${path}" != *$'\n'* && "${path}" != *$'\r'* ]] || return 0

  trimmed="$(trim_trailing_slash "${path}")"
  [[ "${trimmed}" != "/" ]] || return 0
  case "${trimmed}/" in
    *"/./"*|*"/../"*) return 0 ;;
  esac
  return 1
}

path_has_symlink_ancestor() {
  local path="$1"
  local remainder
  local component
  local current=""

  remainder="${path#/}"
  while [[ -n "${remainder}" ]]; do
    if [[ "${remainder}" == */* ]]; then
      component="${remainder%%/*}"
      remainder="${remainder#*/}"
    else
      component="${remainder}"
      remainder=""
    fi
    [[ -n "${component}" ]] || continue
    current="${current}/${component}"
    [[ ! -L "${current}" ]] || return 0
  done
  return 1
}

canonical_existing_dir() {
  local path="$1"

  [[ -d "${path}" && ! -L "${path}" ]] || return 1
  (cd "${path}" >/dev/null 2>&1 && pwd -P)
}

validate_safe_directory_path() {
  local path="$1"
  local canonical

  if path_has_unsafe_alias "${path}" || path_has_symlink_ancestor "${path}"; then
    legacy_cleanup_error "refusing unsafe directory path: ${path}"
    return 1
  fi
  path="$(trim_trailing_slash "${path}")"
  if [[ -e "${path}" && ! -d "${path}" ]]; then
    legacy_cleanup_error "path is not a directory: ${path}"
    return 1
  fi

  [[ -d "${path}" ]] || return 0
  canonical="$(canonical_existing_dir "${path}")" || return 1
  if [[ "${canonical}" != "${path}" ]]; then
    legacy_cleanup_error "directory is not canonical: ${path}"
    return 1
  fi
}

ensure_safe_directory() {
  local path="$1"

  validate_safe_directory_path "${path}" || return 1
  path="$(trim_trailing_slash "${path}")"

  mkdir -p "${path}" || return 1
  validate_safe_directory_path "${path}"
}

validate_link_directory() {
  local link_dir="$1"
  local canonical
  local existing_parent
  local owner_uid

  if path_has_unsafe_alias "${link_dir}" || path_has_symlink_ancestor "${link_dir}"; then
    legacy_cleanup_error "refusing unsafe command-link directory: ${link_dir}"
    return 1
  fi
  link_dir="$(trim_trailing_slash "${link_dir}")"

  if [[ -e "${link_dir}" && ! -d "${link_dir}" ]]; then
    legacy_cleanup_error "command-link path is not a directory: ${link_dir}"
    return 1
  fi
  if [[ -d "${link_dir}" ]]; then
    canonical="$(canonical_existing_dir "${link_dir}")" || {
      legacy_cleanup_error "cannot resolve command-link directory safely: ${link_dir}"
      return 1
    }
    if [[ "${canonical}" != "${link_dir}" ]]; then
      legacy_cleanup_error "command-link directory is not canonical: ${link_dir}"
      return 1
    fi
  fi

  if [[ "${link_dir}" != "/usr/local/bin" ]]; then
    if [[ ! -d "${link_dir}" || ! -w "${link_dir}" ]]; then
      legacy_cleanup_error \
        "custom command-link directory must already exist and be writable: ${link_dir}"
      return 1
    fi
    return 0
  fi

  existing_parent="${link_dir}"
  while [[ ! -e "${existing_parent}" ]]; do
    existing_parent="$(dirname "${existing_parent}")"
  done
  if [[ -w "${existing_parent}" ]]; then
    return 0
  fi

  owner_uid="$(stat -f '%u' "${existing_parent}" 2>/dev/null)" || {
    legacy_cleanup_error "cannot verify ownership of ${existing_parent}"
    return 1
  }
  if [[ "${owner_uid}" != "0" ]]; then
    legacy_cleanup_error \
      "refusing sudo for a command-link directory not rooted in a root-owned path: ${link_dir}"
    return 1
  fi
}

authorize_link_directory() {
  local link_dir="$1"

  validate_link_directory "${link_dir}" || return 1
  link_dir="$(trim_trailing_slash "${link_dir}")"
  if [[ ! -w "${link_dir}" ]]; then
    [[ "${link_dir}" == "/usr/local/bin" ]] || return 1
    echo "Administrator permission is required for command links in ${link_dir}."
    sudo -v
  fi
}

link_points_into_dir() {
  local link_path="$1"
  local root="$2"
  local target
  local expected_target

  if path_has_unsafe_alias "${root}"; then
    return 1
  fi
  root="$(trim_trailing_slash "${root}")"
  [[ -L "${link_path}" ]] || return 1
  target="$(readlink "${link_path}")" || return 1
  expected_target="${root}/$(basename "${link_path}")"
  [[ "${target}" == "${expected_target}" ]]
}

remove_link_if_points_into() {
  local link_path="$1"
  local root="$2"
  local link_dir

  link_points_into_dir "${link_path}" "${root}" || return 0
  link_dir="$(dirname "${link_path}")"
  validate_link_directory "${link_dir}" || return 1
  if [[ -w "${link_dir}" ]]; then
    rm -f "${link_path}"
  else
    [[ "${link_dir}" == "/usr/local/bin" ]] || return 1
    sudo rm -f "${link_path}"
  fi
}

ensure_link_available() {
  local link_path="$1"
  local managed_target="$2"

  if [[ -z "${managed_target}" ]]; then
    legacy_cleanup_error "refusing to manage a command link without a target"
    return 1
  fi
  if [[ ! -e "${link_path}" && ! -L "${link_path}" ]]; then
    return 0
  fi
  if [[ -L "${link_path}" && "$(readlink "${link_path}")" == "${managed_target}" ]]; then
    return 0
  fi
  legacy_cleanup_error "refusing to replace existing command path: ${link_path}"
  return 1
}

install_managed_link() {
  local link_path="$1"
  local managed_target="$2"
  local link_dir

  link_dir="$(dirname "${link_path}")"
  validate_link_directory "${link_dir}" || return 1
  ensure_link_available "${link_path}" "${managed_target}" || return 1
  if [[ -L "${link_path}" && "$(readlink "${link_path}")" == "${managed_target}" ]]; then
    return 0
  fi

  if [[ -w "${link_dir}" ]]; then
    ln -s "${managed_target}" "${link_path}"
  else
    [[ "${link_dir}" == "/usr/local/bin" ]] || return 1
    sudo mkdir -p "${link_dir}"
    validate_link_directory "${link_dir}" || return 1
    sudo ln -s "${managed_target}" "${link_path}"
  fi
}

xml_escape_for_sed_replacement() {
  local value="$1"
  local escaped

  escaped="$(
    printf '%s' "${value}" \
      | sed -e 's/&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g'
  )"
  printf '%s' "${escaped}" | sed -e 's/[\\&#]/\\&/g'
}

write_launchagent_plist() {
  local template_path="$1"
  local destination_path="$2"
  local daemon_path="$3"
  local log_dir="$4"
  local app_support_dir="$5"
  local destination_dir
  local temporary_path
  local escaped_daemon
  local escaped_log_dir
  local escaped_app_support

  destination_dir="$(dirname "${destination_path}")"
  if path_has_unsafe_alias "${destination_dir}" \
    || path_has_symlink_ancestor "${destination_dir}" \
    || [[ ! -d "${destination_dir}" ]] \
    || [[ ! -w "${destination_dir}" ]]; then
    legacy_cleanup_error "refusing unsafe LaunchAgent directory: ${destination_dir}"
    return 1
  fi
  if [[ -L "${template_path}" || ! -f "${template_path}" ]]; then
    legacy_cleanup_error "LaunchAgent template is not a regular file: ${template_path}"
    return 1
  fi
  if [[ -d "${destination_path}" && ! -L "${destination_path}" ]]; then
    legacy_cleanup_error "LaunchAgent destination is a directory: ${destination_path}"
    return 1
  fi

  temporary_path="$(mktemp "${destination_path}.tmp.XXXXXX")" || return 1
  escaped_daemon="$(xml_escape_for_sed_replacement "${daemon_path}")"
  escaped_log_dir="$(xml_escape_for_sed_replacement "${log_dir}")"
  escaped_app_support="$(xml_escape_for_sed_replacement "${app_support_dir}")"

  if ! sed \
    -e "s#__LINKERD_PATH__#${escaped_daemon}#g" \
    -e "s#__LOG_DIR__#${escaped_log_dir}#g" \
    -e "s#__APP_SUPPORT_DIR__#${escaped_app_support}#g" \
    "${template_path}" > "${temporary_path}"; then
    rm -f "${temporary_path}"
    return 1
  fi
  if ! chmod 0644 "${temporary_path}" \
    || ! mv -f -h "${temporary_path}" "${destination_path}"; then
    rm -f "${temporary_path}"
    return 1
  fi
}

validate_cleanup_inputs() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"
  local canonical_home

  if path_has_unsafe_alias "${user_home}" \
    || path_has_symlink_ancestor "${user_home}" \
    || [[ ! -d "${user_home}" ]]; then
    legacy_cleanup_error "refusing legacy cleanup for unsafe HOME: ${user_home}"
    return 1
  fi
  user_home="$(trim_trailing_slash "${user_home}")"
  canonical_home="$(canonical_existing_dir "${user_home}")" || {
    legacy_cleanup_error "cannot resolve HOME safely: ${user_home}"
    return 1
  }
  if [[ "${canonical_home}" == "/" || "${canonical_home}" != "${user_home}" ]]; then
    legacy_cleanup_error "refusing non-canonical HOME: ${user_home}"
    return 1
  fi
  if [[ ! "${user_uid}" =~ ^[0-9]+$ ]] \
    || [[ "${user_uid}" == "0" ]] \
    || [[ "${user_uid}" != "$(id -u)" ]]; then
    legacy_cleanup_error "refusing legacy cleanup for invalid user id: ${user_uid}"
    return 1
  fi
  validate_link_directory "${link_dir}"
}

validate_legacy_parent_paths() {
  local user_home="$1"
  local library_dir="${user_home}/Library"
  local support_root="${library_dir}/Application Support"
  local legacy_root="${support_root}/QuickSync"
  local path

  for path in "${library_dir}" "${support_root}" "${legacy_root}"; do
    if [[ -L "${path}" ]]; then
      legacy_cleanup_error "refusing legacy cleanup through symlink path: ${path}"
      return 1
    fi
    if [[ -e "${path}" && ! -d "${path}" ]]; then
      legacy_cleanup_error "legacy cleanup path is not a directory: ${path}"
      return 1
    fi
  done
}

validate_managed_file() {
  local path="$1"

  if [[ -L "${path}" || ! -f "${path}" ]]; then
    legacy_cleanup_error "refusing to remove non-regular legacy artifact: ${path}"
    return 1
  fi
}

validate_legacy_directory_entries() {
  local directory="$1"
  local kind="$2"
  local entry
  local name
  local allowed

  [[ -d "${directory}" ]] || return 0
  for entry in "${directory}"/* "${directory}"/.[!.]* "${directory}"/..?*; do
    [[ -e "${entry}" || -L "${entry}" ]] || continue
    name="$(basename "${entry}")"
    allowed=0
    case "${kind}:${name}" in
      bin:qs|bin:qsd|logs:qsd.out.log|logs:qsd.err.log|logs:qsyncd.out.log|logs:qsyncd.err.log)
        allowed=1
        ;;
      manifests:*.json|rules:*.ignore) allowed=1 ;;
    esac
    if [[ "${allowed}" != "1" ]]; then
      legacy_cleanup_error "refusing to remove unrecognized legacy content: ${entry}"
      return 1
    fi
    validate_managed_file "${entry}" || return 1
  done
}

legacy_database_allows_cleanup() {
  local legacy_root="$1"
  local state_db="${legacy_root}/state.sqlite"
  local escaped_root
  local protected_count
  local query

  [[ -e "${state_db}" ]] || return 0
  validate_managed_file "${state_db}" || return 1
  if ! command -v sqlite3 >/dev/null 2>&1; then
    legacy_cleanup_error "sqlite3 is required to verify legacy source and target paths"
    return 1
  fi

  escaped_root="$(printf '%s' "${legacy_root}" | sed "s/'/''/g")"
  query="SELECT COUNT(*) FROM items WHERE local_path = '${escaped_root}' OR cloud_path = '${escaped_root}' OR substr(local_path, 1, length('${escaped_root}') + 1) = '${escaped_root}/' OR substr(cloud_path, 1, length('${escaped_root}') + 1) = '${escaped_root}/';"
  protected_count="$(sqlite3 -noheader "${state_db}" "${query}" 2>/dev/null)" || {
    legacy_cleanup_error "cannot verify legacy associations in ${state_db}"
    return 1
  }
  if [[ ! "${protected_count}" =~ ^[0-9]+$ ]]; then
    legacy_cleanup_error "legacy association check returned an invalid result"
    return 1
  fi
  if [[ "${protected_count}" != "0" ]]; then
    legacy_cleanup_error \
      "legacy state contains a source or target inside ${legacy_root}; move it out before retrying"
    return 1
  fi
}

validate_legacy_layout() {
  local legacy_root="$1"
  local entry
  local name

  [[ -d "${legacy_root}" ]] || return 0
  legacy_database_allows_cleanup "${legacy_root}" || return 1

  for entry in "${legacy_root}"/* "${legacy_root}"/.[!.]* "${legacy_root}"/..?*; do
    [[ -e "${entry}" || -L "${entry}" ]] || continue
    name="$(basename "${entry}")"
    case "${name}" in
      state.sqlite|state.sqlite-shm|state.sqlite-wal|config.json|qsd.lock|qsyncd.lock|.DS_Store)
        validate_managed_file "${entry}" || return 1
        ;;
      bin|logs|manifests|rules|tmp)
        if [[ -L "${entry}" || ! -d "${entry}" ]]; then
          legacy_cleanup_error "refusing unsafe legacy directory: ${entry}"
          return 1
        fi
        ;;
      *)
        legacy_cleanup_error "refusing to remove unrecognized legacy content: ${entry}"
        return 1
        ;;
    esac
  done

  validate_legacy_directory_entries "${legacy_root}/bin" bin || return 1
  validate_legacy_directory_entries "${legacy_root}/logs" logs || return 1
  validate_legacy_directory_entries "${legacy_root}/manifests" manifests || return 1
  validate_legacy_directory_entries "${legacy_root}/rules" rules || return 1
  validate_legacy_directory_entries "${legacy_root}/tmp" tmp || return 1
}

validate_legacy_cleanup() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"
  local legacy_root

  validate_cleanup_inputs "${user_home}" "${link_dir}" "${user_uid}" || return 1
  user_home="$(trim_trailing_slash "${user_home}")"
  validate_legacy_parent_paths "${user_home}" || return 1
  legacy_root="${user_home}/Library/Application Support/QuickSync"
  validate_legacy_layout "${legacy_root}"
}

legacy_managed_links_present() {
  local user_home="$1"
  local link_dir="$2"
  local legacy_bin

  user_home="$(trim_trailing_slash "${user_home}")"
  link_dir="$(trim_trailing_slash "${link_dir}")"
  legacy_bin="${user_home}/Library/Application Support/QuickSync/bin"
  link_points_into_dir "${link_dir}/qs" "${legacy_bin}" \
    || link_points_into_dir "${link_dir}/qsd" "${legacy_bin}"
}

launchctl_reports_missing_service() {
  local output="$1"

  case "${output}" in
    *"Could not find service"*|*"service not found"*|*"Could not find specified service"*)
      return 0
      ;;
  esac
  return 1
}

stop_launchagent() {
  local user_uid="$1"
  local label="$2"
  local plist_path="$3"
  local service_target="gui/${user_uid}/${label}"
  local output

  command -v launchctl >/dev/null 2>&1 || return 0

  if output="$(launchctl print "${service_target}" 2>&1)"; then
    if ! launchctl bootout "${service_target}" >/dev/null 2>&1; then
      if [[ ! -f "${plist_path}" || -L "${plist_path}" ]] \
        || ! launchctl bootout "gui/${user_uid}" "${plist_path}" >/dev/null 2>&1; then
        legacy_cleanup_error "could not stop LaunchAgent ${label}"
        return 1
      fi
    fi
  elif launchctl_reports_missing_service "${output}"; then
    return 0
  else
    legacy_cleanup_error "could not determine LaunchAgent state for ${label}: ${output}"
    return 1
  fi

  if output="$(launchctl print "${service_target}" 2>&1)"; then
    legacy_cleanup_error "LaunchAgent ${label} is still running"
    return 1
  fi
  if ! launchctl_reports_missing_service "${output}"; then
    legacy_cleanup_error "could not verify LaunchAgent stopped for ${label}: ${output}"
    return 1
  fi
}

stop_legacy_daemon() {
  local user_home="$1"
  local user_uid="$2"
  local legacy_plist="${user_home}/Library/LaunchAgents/com.quicksync.qsd.plist"

  stop_launchagent "${user_uid}" "com.quicksync.qsd" "${legacy_plist}"
}

remove_managed_file() {
  local path="$1"

  [[ -e "${path}" || -L "${path}" ]] || return 0
  validate_managed_file "${path}" || return 1
  rm -f "${path}"
}

remove_legacy_directory_files() {
  local directory="$1"
  local kind="$2"
  local entry

  [[ -d "${directory}" ]] || return 0
  validate_legacy_directory_entries "${directory}" "${kind}" || return 1
  for entry in "${directory}"/* "${directory}"/.[!.]* "${directory}"/..?*; do
    [[ -e "${entry}" || -L "${entry}" ]] || continue
    remove_managed_file "${entry}" || return 1
  done
  rmdir "${directory}"
}

remove_legacy_artifacts() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"
  local legacy_root
  local legacy_plist
  local name

  validate_legacy_cleanup "${user_home}" "${link_dir}" "${user_uid}" || return 1
  user_home="$(trim_trailing_slash "${user_home}")"
  link_dir="$(trim_trailing_slash "${link_dir}")"
  legacy_root="${user_home}/Library/Application Support/QuickSync"
  legacy_plist="${user_home}/Library/LaunchAgents/com.quicksync.qsd.plist"

  if [[ -L "${legacy_plist}" || -f "${legacy_plist}" ]]; then
    rm -f "${legacy_plist}"
  elif [[ -e "${legacy_plist}" ]]; then
    legacy_cleanup_error "refusing to remove non-file legacy plist: ${legacy_plist}"
    return 1
  fi

  remove_link_if_points_into "${link_dir}/qs" "${legacy_root}/bin" || return 1
  remove_link_if_points_into "${link_dir}/qsd" "${legacy_root}/bin" || return 1

  [[ -d "${legacy_root}" ]] || return 0
  for name in \
    state.sqlite state.sqlite-shm state.sqlite-wal config.json qsd.lock qsyncd.lock .DS_Store; do
    remove_managed_file "${legacy_root}/${name}" || return 1
  done
  remove_legacy_directory_files "${legacy_root}/bin" bin || return 1
  remove_legacy_directory_files "${legacy_root}/logs" logs || return 1
  remove_legacy_directory_files "${legacy_root}/manifests" manifests || return 1
  remove_legacy_directory_files "${legacy_root}/rules" rules || return 1
  remove_legacy_directory_files "${legacy_root}/tmp" tmp || return 1
  rmdir "${legacy_root}"
}

cleanup_legacy_install() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"

  validate_legacy_cleanup "${user_home}" "${link_dir}" "${user_uid}" || return 1
  stop_legacy_daemon "$(trim_trailing_slash "${user_home}")" "${user_uid}" || return 1
  remove_legacy_artifacts "${user_home}" "${link_dir}" "${user_uid}"
}
