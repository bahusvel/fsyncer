# fsyncer
VFS based distributed file system replication

# Compiling
Don't forget to checkout dssc `git submodule update --init`
On an ubuntu 18.04 you need:
* clang
* cmake
* libfuse 3.2.x (cannot apt-get, must install from sources https://github.com/libfuse/libfuse/releases)
  for which you need:
    * meson