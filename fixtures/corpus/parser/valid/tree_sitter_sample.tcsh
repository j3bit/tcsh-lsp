#!/bin/tcsh
# Sample tcsh/csh surface syntax for parser smoke testing.
set path = ( /bin /usr/bin )
setenv EDITOR vim
alias ll 'ls -l'
@ count = 1 + 2 * 3
if ( -e ~/.tcshrc && $count >= 3 ) then
  echo "ready $USER"
else
  echo `hostname`
endif
foreach item ( *.c *.h )
  echo $item:q
end
while ( $count > 0 )
  @ count--
end
switch ( $argv[1] )
case *.c:
  echo source
  breaksw
default:
  goto done
endsw
cat << EOF_MARKER
heredoc body is documented through scanner gate
EOF_MARKER
repeat 2 echo again
onintr cleanup
cleanup:
echo !$ %1
