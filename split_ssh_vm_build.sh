#! /bin/bash
#
# Vault Handler Build Script
#
# # # # # # # # # # # # # # # 

# Script Arguments #
PKG_NAME="$1"
USER_U="$2"
IS_VAULT="$3"
# ~~~~~~~~~~~~~~~~ #

err_exit() {
  echo "$BASH_COMMAND @: $LINENO"
  exit $?
}

trap err_exit ERR

HOME_U="/home/$USER_U"
QINC_DIR="$HOME_U/QubesIncoming/dom0"
ZIP_PATH="$QINC_DIR/qss.zip"
PROJ_DIR="$HOME_U/qss"
PROJ_MANIFEST="$PROJ_DIR/$PKG_NAME/Cargo.toml"
AOUT="$PROJ_DIR/target/debug/$PKG_NAME"
LOGS_DIR="$HOME_U/.local/state/split-ssh"
VAULT_RPC_PATH="/etc/qubes-rpc/qubes.SplitSSHAgent"

if [[ -d $PROJ_DIR ]]; then
  rm -rf $PROJ_DIR 
fi  

if [[ -d $LOGS_DIR ]]; then
  rm -rf $LOGS_DIR 
fi

mkdir $PROJ_DIR
unzip $ZIP_PATH -d $PROJ_DIR
cargo build --manifest-path $PROJ_MANIFEST
chmod 755 $AOUT

if [ $IS_VAULT == 1 ]; then 
  chown $USER_U:$USER_U $AOUT
  mv $AOUT $VAULT_RPC_PATH 
fi

rm -f $0
