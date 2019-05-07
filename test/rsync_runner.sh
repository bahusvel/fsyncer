rm -rf sico || true
mkfifo sico || true
rm -rf soci || true
mkfifo soci || true

rsync -avhAX --delete -e `realpath test/rsync_pipe.sh` .fsyncer-test_src/ :. &
rsync --server -vlogDtpAXre.iLsfxC --delete . `realpath test_dst` < soci > sico 