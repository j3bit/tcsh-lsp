#!/bin/tcsh
# Showcase tcsh/csh surface syntax for tree-sitter-tcsh.
# It is intentionally parser-focused: syntax trees, not shell execution semantics.

# Builtins with structured arguments, quoted strings, substitutions, and modifiers.
set path = ( /bin /usr/bin /usr/local/bin )
set files = ( *.c *.h src/[a-z]*.{c,h} )
setenv EDITOR nvim
alias ll 'ls -l --color=auto'
complete cd 'p/1/d/'
bindkey '^R' history-search-backward
limit coredumpsize 0
umask 022
sched +00:05 echo scheduled

# Arithmetic, assignment/update operators, precedence, and ternary expressions.
@ total = 1 + 2 * 3
@ total++
@ total -= 2
@ choice = $total > 3 ? 1 : 0

# Command graph: lists, pipelines, and/or, background suffix, all as syntax.
echo start ; echo middle && echo ok || echo fail
printf "%s\\n" "$USER:q" |& grep '*'
sleep 1 &

# Redirections, including clobber/append and stderr forms.
echo stdin < /tmp/input.txt
echo stdout >! /tmp/tcsh-showcase.out
echo append >> /tmp/tcsh-showcase.out
echo append-clobber >>! /tmp/tcsh-showcase.out
echo both >& /tmp/tcsh-showcase.err
echo both-clobber >&! /tmp/tcsh-showcase.err
echo both-append >>& /tmp/tcsh-showcase.err
echo both-append-clobber >>&! /tmp/tcsh-showcase.err
cat << EOF_MARKER
heredoc body stays parser-readable; scanner design governs body attachment.
EOF_MARKER

# Parenthesized commands and command-status expressions.
( echo grouped ; echo command-list )
if ( { echo status-probe } ) echo command-status-ok

# File tests, command substitutions, history substitutions, glob patterns, and job specs.
if ( -e ~/.tcshrc && $total >= 1 ) then
  echo "home=${HOME}:h host=`hostname` last=!! arg=!:1"
else if ( *.c =~ *.c ) then
  echo glob-match src/[a-z]*.{c,h}
else
  echo fallback !$ %1
endif

# Foreach with variable substitution, subscript, modifiers, escapes, and line continuation.
foreach item ( $files )
  echo item=$item:q argv1=$argv[1] escaped=foo\ bar continued=one\
  two
end

# While loop with @ update syntax.
while ( $total > 0 )
  echo countdown $total
  @ total--
end

# Switch/case/default/breaksw and labels/goto/onintr.
onintr cleanup
switch ( $argv[1] )
case *.c:
  echo c-source
  breaksw
case *.h:
  echo header
  breaksw
default:
  goto cleanup
endsw

repeat 2 echo repeated

cleanup:
jobs %+
fg %1
bg %-
kill -l
wait
where tcsh
which nvim
history 5
