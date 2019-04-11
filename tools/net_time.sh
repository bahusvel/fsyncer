#!/bin/sh
IFACE=$1
CMD=$2
shift
shift

START_BYTES=$(cat /sys/class/net/$IFACE/statistics/tx_bytes)
START_TIME=$(($(date +%s%N)/1000000))

$CMD $@

END_TIME=$(($(date +%s%N)/1000000))
END_BYTES=$(cat /sys/class/net/$IFACE/statistics/tx_bytes)

DIFF_BYTES=$(expr $END_BYTES - $START_BYTES)
echo Sent $DIFF_BYTES bytes \(`expr $DIFF_BYTES / 1024 / 1024`MB\) in `echo "scale=3; ($END_TIME-$START_TIME) / 1000.0" | bc` seconds