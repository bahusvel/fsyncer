#!/bin/sh
IFACE=$1
CMD=$2
shift
shift

START_BYTES=$(cat /sys/class/net/$IFACE/statistics/tx_bytes)
START_TIME=$(date +%s)

$CMD $@

END_TIME=$(date +%s)
END_BYTES=$(cat /sys/class/net/$IFACE/statistics/tx_bytes)

DIFF_BYTES=$(expr $END_BYTES - $START_BYTES)
echo Sent $DIFF_BYTES bytes \(`expr $DIFF_BYTES / 1024 / 1024`MB\) in `expr $END_TIME - $START_TIME` seconds
