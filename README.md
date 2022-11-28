# Peek Poke Threading Example

This is the way used in [webrender](https://github.com/servo/webrender) for sending struct to other thread.

## How it works

I'm using [peek-poke](https://github.com/servo/webrender/tree/8827e79bd44b2f8a0cfef80a5123d97cf98c65ed/peek-poke) to send some value to another thread through channel.

This gets the pointer from each values(poke).
Also this gets each values from the pointer(peek).

We can share some value between thread efficiently.
