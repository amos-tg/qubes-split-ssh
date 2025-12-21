# /bin/bash!
#
# Script for automated testing of 
# qubes-split-ssh workspace
#
# Change these if personal values differ:
# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ #

COMPILER_VM="dev"
COMPILER_VM_USER="user"
PROJECT_DIR="/home/$COMPILER_VM_USER/projects/rust/qubes-split-ssh"


VAULT_VM_USER="user"
VAULT_VM="ssh-vault"


CLIENT_VM_USER="user"
CLIENT_VM="ssh-client-dvm"

# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ #

err_exit() {
  local ECODE=$?
  rm -rf $WORKING_DIR &> /dev/null || true;
  echo "$BASH_COMMAND @: $LINENO"
  exit $ECODE
}

trap err_exit ERR

LOCAL_SCRIPT_PATH=$0
LOCAL_VAULT_STARTUP_SCRIPT_PATH=$(dirname $0)
QINC_SCRIPT_PATH="/QubesIncoming/dom0/split_ssh_vm_build.sh"

VAULT_QINC="/home/$VAULT_VM_USER/QubesIncoming/dom0"
VAULT_ZIP_PATH="$VAULT_QINC/qss.zip"
VAULT_SCRIPT_PATH="$VAULT_QINC/split_ssh_vm_build.sh"
VAULT_STARTUP_SCRIPT_PATH="$VAULT_QINC/vault_startup.sh"
VAULT_PKG_NAME="vault_handler"

CLIENT_SCRIPT_PATH="$CLIENT_QINC/split_ssh_vm_build.sh"
CLIENT_QINC="/home/$CLIENT_VM_USER/QubesIncoming/dom0"
CLIENT_ZIP_PATH="$CLIENT_QINC/qss.zip"
CLIENT_PKG_NAME="client_handler"


WORKING_DIR="$(mktemp --directory)"

qvm-run --pass-io -u $COMPILER_VM_USER $COMPILER_VM "\
cd $PROJECT_DIR && \
zip -r qss.zip ./* -x 'target/***' '.git/***' '.gitignore' \
'notes/***' 'split_ssh_debug.sh' 'split_ssh_vm_build.sh' && \
cat qss.zip" > \
$WORKING_DIR/qss.zip

qvm-run $COMPILER_VM "rm -f $PROJECT_DIR/qss.zip"

qvm-run -u $CLIENT_VM_USER $CLIENT_VM "\
rm -f $CLIENT_ZIP_PATH $CLIENT_SCRIPT_PATH"

qvm-copy-to-vm $CLIENT_VM $WORKING_DIR/qss.zip
qvm-copy-to-vm $CLIENT_VM $LOCAL_SCRIPT_PATH

qvm-run -u $VAULT_VM_USER  $VAULT_VM "\
rm -f $VAULT_ZIP_PATH $VAULT_SCRIPT_PATH"

qvm-copy-to-vm $VAULT_VM $WORKING_DIR/qss.zip
qvm-copy-to-vm $VAULT_VM $LOCAL_SCRIPT_PATH

qvm-run -u root --pass-io $CLIENT_VM "\
$CLIENT_SCRIPT_PATH $CLIENT_PKG_NAME $CLIENT_VM_USER 0"

qvm-run -u root --pass-io $VAULT_VM "\
$VAULT_SCRIPT_PATH $VAULT_PKG_NAME $VAULT_VM_USER 1"

rm -rf $WORKING_DIR &> /dev/null
