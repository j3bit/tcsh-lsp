# comment
start:
set name = "world"
echo '$literal' "$name" `hostname` *.csh ${HOME} $name > out | grep csh
( cd /tmp ; echo $(pwd) )
