# /bin/bash!
#
# Script for automated testing of 
# qubes-split-ssh workspace functionalities
# Change these if personal values differ:
# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ #
LOCAL_SCRIPT_PATH="/home/shannel/debug_scripts/split_ssh_vm_build.sh";
LOCAL_VAULT_STARTUP_SCRIPT_PATH="/home/shannel/debug_scripts/vault_startup.sh";
QINC_SCRIPT_PATH="/QubesIncoming/dom0/split_ssh_vm_build.sh";


COMPILER_VM="dev";
COMPILER_VM_USER="user";
PROJECT_DIR="/home/$COMPILER_VM_USER/projects/rust/qubes-split-ssh";


VAULT_VM_USER="user";
VAULT_QINC="/home/$VAULT_VM_USER/QubesIncoming/dom0";
VAULT_ZIP_PATH="$VAULT_QINC/qss.zip";
VAULT_SCRIPT_PATH="$VAULT_QINC/split_ssh_vm_build.sh";
VAULT_STARTUP_SCRIPT_PATH="$VAULT_QINC/vault_startup.sh";
VAULT_VM="ssh-vault";
VAULT_PKG_NAME="vault_handler";


CLIENT_VM_USER="user";
CLIENT_QINC="/home/$CLIENT_VM_USER/QubesIncoming/dom0";
CLIENT_ZIP_PATH="$CLIENT_QINC/qss.zip";
CLIENT_SCRIPT_PATH="$CLIENT_QINC/split_ssh_vm_build.sh";
CLIENT_VM="ssh-client-dvm";
CLIENT_PKG_NAME="client_handler";
# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ #

WORKING_DIR="$(mktemp --directory)" || exit 1;

qvm-run --pass-io -u $COMPILER_VM_USER $COMPILER_VM "
cd $PROJECT_DIR &&
zip -r qss.zip ./* -x 'target/***' '.git/***' '.gitignore' \
'notes/***' 'split_ssh_debug.sh' 'split_ssh_vm_build.sh' &&
cat qss.zip" > $WORKING_DIR/qss.zip || exit 2;
qvm-run $COMPILER_VM "rm -f $PROJECT_DIR/qss.zip" || exit 3;

qvm-run -u $CLIENT_VM_USER $CLIENT_VM "
rm -f $CLIENT_ZIP_PATH $CLIENT_SCRIPT_PATH" || exit 8; 
qvm-copy-to-vm $CLIENT_VM $WORKING_DIR/qss.zip || exit 4;
qvm-copy-to-vm $CLIENT_VM $LOCAL_SCRIPT_PATH || exit 5;

qvm-run -u $VAULT_VM_USER  $VAULT_VM "
rm -f $VAULT_ZIP_PATH $VAULT_SCRIPT_PATH" || exit 9;
qvm-copy-to-vm $VAULT_VM $WORKING_DIR/qss.zip || exit 6;
qvm-copy-to-vm $VAULT_VM $LOCAL_SCRIPT_PATH || exit 7; 

qvm-run -u root --pass-io $CLIENT_VM "
$CLIENT_SCRIPT_PATH $CLIENT_PKG_NAME $CLIENT_VM_USER 0" || exit 8;

qvm-run -u root --pass-io $VAULT_VM "
$VAULT_SCRIPT_PATH $VAULT_PKG_NAME $VAULT_VM_USER 1" || exit 9;

rm -rf $WORKING_DIR &> /dev/null || exit 10; 

exit 0;
