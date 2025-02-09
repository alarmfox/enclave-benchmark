System requirements 
===================
This is setup page

Host requirements
-----------------


OS Dependencies
---------------

Host setup
^^^^^^^^^^

Building Gramine
^^^^^^^^^^^^^^^^

We need to build Gramine from source to get access to debug information at runtime. 

.. code:: sh

  meson setup build/ \
    --buildtype=debugoptimized \
    -Ddirect=enabled \ 
    -Dsgx=enabled \
    -Ddcap=enabled

Docker 
^^^^^^

