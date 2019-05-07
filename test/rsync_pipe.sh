#! /bin/bash
cat /home/denislavrov/Documents/Developing/fsyncer/sico & cat > /home/denislavrov/Documents/Developing/fsyncer/soci &
wait -n
pkill -P $$