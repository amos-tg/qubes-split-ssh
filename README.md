Cargo Workspace: I haven't tried it yet other than a few ssh-add -l tests which worked

- client_handler & vault_handler: rust protocol's opposing ends communication programs. They could be one binary, which would be better; I'll make this change before I put the program on my personal qubes install because I don't want to juggle multiple binaries.

- socket_stdinout: lib used by client_handler & vault_handler for handling communications. There are some poor choices I will fix sooner or later, but the speed is tolerable and multiple times faster than the previous versions.

The workspace handles proxying ssh-agent queries over qrexec-client-vm; this allows you to keep your keys on an offline VM.

You might be able to expose the ssh-agent proxy over the network with ssh's built in agent forwarding plus the proxy on a dedicated non key vault vm plus some firewall configuration. 

To the best of my knowledge, an qubes.SplitSSHAgent RPC priviledged VM has no way to gather keys on the key vault filesystem which aren't already loaded into the agent. 

I'm not paranoid enough to spend time on implementing this but If you wanted to it would be feasable to proxy different agents with different keys loaded to different VMs or to network exposure; so you can provide different keys at the same time without exposing them to all the connections. 
