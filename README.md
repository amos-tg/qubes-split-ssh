This is a work in progress
 
There are multiple binaries produced when you build the workspace:
 
- split_ssh_setup: This, as of the moment, implements socat based split-ssh when you run it in dom0 with qvm commands and salt files. I am planning on adding an option where you can choose between the socat based communications and the rust based communications which I rolled myself.The interface I made is not very pretty so I plan on fixing that up later. Last time I ran this it worked fine. 

- client_handler & vault_handler: rust protocol's opposing ends communication programs. 

- socket_stdinout: lib used by client_handler & vault_handler for handling communications. This was pretty slow last time I checked but I haven't run an optimized version so it may be faster than I think.
