# /bin/bash!
#
# Script for automated testing of qubes-split-ssh workspace functionalities
#
#
# Change these if personal values differ:
# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ #
COMPILER_VM="dev" 
COMPILER_VM_USER="user"
PROJECT_DIR="/home/user/projects/rust/qubes-split-ssh"

SSH_VAULT_VM="ssh-vault" 
SSH_VAULT_VM_USER="user"

SSH_CLIENT_VM="ssh-client-dvm"
SSH_CLIENT_VM_USER="user"
# ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ #
#
#

WORKING_DIR="$(mktemp --directory)" || exit 1;

qvm-run --pass-io -u $COMPILER_VM_USER $COMPILER_VM "\
cd $PROJECT_DIR && \
cargo build --workspace && \
cat ./target/debug/client_handler \
" > $WORKING_DIR/client_handler || exit 2;

qvm-run --pass-io -u $COMPILER_VM_USER $COMPILER_VM "\
cd $PROJECT_DIR && \
cat ./target/debug/vault_handler \
" > $WORKING_DIR/vault_handler || exit 3;

qvm-run -u user $SSH_CLIENT_VM "rm -rf ~/QubesIncoming/dom0/client_handler";
qvm-run -u user $SSH_VAULT_VM "rm -rf ~/QubesIncoming/dom0/vault_handler";

qvm-copy-to-vm $SSH_CLIENT_VM $WORKING_DIR/client_handler || exit 4;
qvm-copy-to-vm $SSH_VAULT_VM $WORKING_DIR/vault_handler || exit 5;

qvm-run -u root $SSH_VAULT_VM "\
rm -rf ~/.local/state/split-ssh;
chmod 755 /home/$SSH_VAULT_VM_USER/QubesIncoming/dom0/vault_handler && \
chown root:root /home/$SSH_VAULT_VM_USER/QubesIncoming/dom0/vault_handler && \
mv \
/home/$SSH_VAULT_VM_USER/QubesIncoming/dom0/vault_handler \
/etc/qubes-rpc/qubes.SplitSSHAgent \
";

qvm-run -u $SSH_CLIENT_VM_USER $SSH_CLIENT_VM "\
  rm -rf ~/.local/state/split-ssh;
  chmod 755 ~/QubesIncoming/dom0/client_handler \
";

rm -rf $WORKING_DIR || exit 8; 

exit 0;
