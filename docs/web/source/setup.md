# Setup
This is setup page

## Building Gramine
We need to build Gramine from source to get access to debug information at runtime. 

```sh
meson setup build/ \
  --buildtype=debugoptimized \
  -Ddirect=enabled \ 
  -Dsgx=enabled \
  -Ddcap=enabled
```

## Host setup

## Docker setup
