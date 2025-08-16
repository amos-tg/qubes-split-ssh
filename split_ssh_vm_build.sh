#! /bin/bash
#
# Vault Handler Build Script
#
# # # # # # # # # # # # # # # 

# Script Arguments #
PKG_NAME="$1";
USER_U="$2";
IS_VAULT="$3"; 
# ~~~~~~~~~~~~~~~~ #

HOME_U="/home/$USER_U";
QINC_DIR="$HOME_U/QubesIncoming/dom0";
SELF_PATH="$QINC_DIR/split_ssh_vm_build.sh";
ZIP_PATH="$QINC_DIR/qss.zip";
PROJ_DIR="$HOME_U/qss";
PROJ_MANIFEST="$PROJ_DIR/$PKG_NAME/Cargo.toml";
AOUT="$PROJ_DIR/target/debug/$PKG_NAME";
LOGS_DIR="$HOME_U/.local/state/split-ssh";
VAULT_RPC_PATH="/etc/qubes-rpc/qubes.SplitSSHAgent";

if [ -d $PROJ_DIR ]; then
  rm -rf $PROJ_DIR; fi  

if [ -d $LOGS_DIR ]; then
  rm -rf $LOGS_DIR; fi

mkdir $PROJ_DIR || exit 3;
unzip $ZIP_PATH -d $PROJ_DIR || true;
cargo build --manifest-path $PROJ_MANIFEST || exit 2;
chmod 755 $AOUT || exit 4

if [ $IS_VAULT == 1 ]; then 
chown root:root $AOUT || exit 5;
mv $AOUT $VAULT_RPC_PATH || exit 6; fi

rm -f $SELF_PATH || exit ;
