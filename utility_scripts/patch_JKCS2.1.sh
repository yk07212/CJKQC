#!/usr/bin/env bash
#
# patch_JKCS2.1.sh - make a JKCS2.1 installation use CJKQC and JSONL databases.
#
# Usage:
#   patch_JKCS2.1.sh [OPTIONS] JKCS2.1_DIR USERSETUP CJKQC_EXECUTABLE
#
#   JKCS2.1_DIR        Root of the JKCS2.1 installation (contains JKCSx/ and TOOLS/)
#   USERSETUP          Path to the .JKCSusersetup(.txt) loaded by the JKCS scripts
#   CJKQC_EXECUTABLE   Path to the compiled CJKQC binary
#
# Options:
#   -n, --dry-run      Report the changes without writing any file
#   --no-backup        Do not create *.cjkqc-patch.bak backups
#   -h, --help         Show this help
#
# What it does:
#   * adds CJKQC_BIN and program_CJKQC to the user setup file
#   * ABCluster collection (JKCS2_explore) writes collectionABC.jsonl via CJKQC
#   * CREST collection (program_CREST in the user setup) writes JSONL via CJKQC
#   * JKCS3_run checks/writes JSONL collections where it used to write PKL
#   * JKCS4_collect and JKcollect use CJKQC + JSONL instead of the bundled JKQC
#   * patches a helper function that carries ABC/CREST energies into the records
#
# The patch is idempotent; running it again only refreshes the CJKQC path.

set -euo pipefail

usage() {
  sed -n '3,24p' "$0" | sed 's/^# \{0,1\}//'
}

DRY_RUN=0
BACKUP=1

while [ $# -gt 0 ]; do
  case "$1" in
    -n|--dry-run) DRY_RUN=1; shift ;;
    --no-backup) BACKUP=0; shift ;;
    -h|--help) usage; exit 0 ;;
    --) shift; break ;;
    -*) echo "error: unknown option: $1" >&2; usage >&2; exit 2 ;;
    *) break ;;
  esac
done

if [ $# -ne 3 ]; then
  echo "error: expected 3 arguments, got $#" >&2
  usage >&2
  exit 2
fi

absolute_path() {
  local path=$1
  case "$path" in
    "~") path=$HOME ;;
    "~/"*) path=$HOME/${path#\~/} ;;
  esac
  if [ -d "$path" ]; then
    ( cd "$path" && pwd -P )
  elif [ -e "$path" ]; then
    local dir base
    dir=$(dirname "$path")
    base=$(basename "$path")
    ( cd "$dir" && printf '%s/%s\n' "$(pwd -P)" "$base" )
  else
    printf '%s\n' "$path"
  fi
}

JKCS_DIR=$(absolute_path "$1")
USERSETUP=$(absolute_path "$2")
CJKQC_BIN=$(absolute_path "$3")

fail() { echo "error: $*" >&2; exit 1; }

command -v perl >/dev/null 2>&1 || fail "perl is required to patch the scripts"

[ -d "$JKCS_DIR" ] || fail "JKCS2.1 directory does not exist: $JKCS_DIR"
[ -f "$USERSETUP" ] || fail "user setup file does not exist: $USERSETUP"
[ -f "$CJKQC_BIN" ] || fail "CJKQC executable does not exist: $CJKQC_BIN"
[ -x "$CJKQC_BIN" ] || fail "CJKQC is not executable: $CJKQC_BIN"

[ -f "$JKCS_DIR/JKCSx/JKCS3_run" ] || fail "not a JKCS2.1 installation: $JKCS_DIR/JKCSx/JKCS3_run is missing"
[ -f "$JKCS_DIR/JKCSx/JKCS4_collect" ] || fail "not a JKCS2.1 installation: $JKCS_DIR/JKCSx/JKCS4_collect is missing"
[ -f "$JKCS_DIR/TOOLS/SCRIPTS/JKcollect" ] || fail "not a JKCS2.1 installation: $JKCS_DIR/TOOLS/SCRIPTS/JKcollect is missing"

export JKCS_DIR USERSETUP CJKQC_BIN DRY_RUN BACKUP

perl - <<'PERL'
use strict;
use warnings;
use File::Copy qw(copy);

my $JKCS_DIR = $ENV{JKCS_DIR};
my $USERSETUP = $ENV{USERSETUP};
my $CJKQC_BIN = $ENV{CJKQC_BIN};
my $DRY_RUN = $ENV{DRY_RUN} eq '1';
my $BACKUP = $ENV{BACKUP} eq '1';

my $JKCS3 = "$JKCS_DIR/JKCSx/JKCS3_run";
my $JKCS4 = "$JKCS_DIR/JKCSx/JKCS4_collect";
my $EXPLORE = "$JKCS_DIR/JKCSx/JKCS2_explore";
my $JKCOLLECT = "$JKCS_DIR/TOOLS/SCRIPTS/JKcollect";

for my $path ($USERSETUP, $JKCS3, $JKCS4, $EXPLORE, $JKCOLLECT) {
    die "error: required file is missing: $path\n" unless -f $path;
}

my %cache;

sub slurp {
    my ($path) = @_;
    return $cache{$path} if $DRY_RUN && exists $cache{$path};
    open my $fh, '<:raw', $path or die "error: cannot read $path: $!\n";
    local $/;
    my $text = <$fh>;
    close $fh;
    return $text;
}

sub ensure_backup {
    my ($path) = @_;
    return unless $BACKUP && !$DRY_RUN;
    my $backup = "$path.cjkqc-patch.bak";
    return if -e $backup;
    copy($path, $backup) or die "error: cannot back up $path: $!\n";
}

sub spew {
    my ($path, $text) = @_;
    if ($DRY_RUN) {
        $cache{$path} = $text;
        return;
    }
    ensure_backup($path);
    open my $fh, '>:raw', $path or die "error: cannot write $path: $!\n";
    print {$fh} $text;
    close $fh or die "error: cannot write $path: $!\n";
}

sub patch {
    my ($path, $old, $new, $label, $all) = @_;
    chomp $old;
    chomp $new;
    my $text = slurp($path);
    if (index($text, $old) < 0) {
        if (index($text, $new) >= 0) {
            print "  skip   $label (already applied)\n";
            return;
        }
        die "error: $label: expected snippet not found in $path\n";
    }
    unless ($all) {
        my $count = () = $text =~ /\Q$old\E/g;
        die "error: $label: snippet found $count times in $path\n" if $count != 1;
    }
    if ($all) {
        $text =~ s/\Q$old\E/$new/g;
    } else {
        $text =~ s/\Q$old\E/$new/;
    }
    spew($path, $text);
    print $DRY_RUN ? "  would  $label\n" : "  patch  $label\n";
}

sub install_block {
    my ($path) = @_;
    my $start = '# >>> CJKQC patcher >>>';
    my $end = '# <<< CJKQC patcher <<<';
    my $block = <<'BLOCK';
# >>> CJKQC patcher >>>
# Added by CJKQC/utility_scripts/patch_JKCS2.1.sh
CJKQC_BIN="__CJKQC_BIN__"
function program_CJKQC {
  "$CJKQC_BIN" "$@"
}
function _jkcs_energy_from_line {
  local line=$1
  local value=""
  if [[ "$line" =~ [Ee]lectronic[_[:space:]]*[Ee]nergy[[:space:]]*[:=][[:space:]]*([-+]?[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?) ]]; then
    value="${BASH_REMATCH[1]}"
  elif [[ "$line" =~ [Ss]tructure[[:space:]]*[Ee]nergy[[:space:]]*[:=][[:space:]]*([-+]?[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?) ]]; then
    value="${BASH_REMATCH[1]}"
  elif [[ "$line" =~ [Aa][Bb][Cc][[:space:]]*[Ee]nergy[[:space:]]*[:=][[:space:]]*([-+]?[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?) ]]; then
    value="${BASH_REMATCH[1]}"
  elif [[ "$line" =~ [Ee]nergy[[:space:]]*[:=][[:space:]]*([-+]?[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?) ]]; then
    value="${BASH_REMATCH[1]}"
  elif [[ "$line" =~ ^[[:space:]]*([-+]?[0-9]+(\.[0-9]*)?([eE][-+]?[0-9]+)?)[[:space:]]*$ ]]; then
    value="${BASH_REMATCH[1]}"
  fi
  printf '%s\n' "$value"
}
function program_CJKQC_energy {
  local db line line2 source log comment energy tmp changed value mode
  for db in "$@"; do
    if [ ! -f "$db" ]; then
      echo "warning: database does not exist: $db" >&2
      continue
    fi
    tmp="${db}.cjkqc_energy.$$.$RANDOM"
    changed=0
    while IFS= read -r line || [ -n "$line" ]; do
      if [ -z "$line" ]; then
        printf '\n' >> "$tmp"
        continue
      fi
      if [[ "$line" == *'"electronic_energy"'* ]]; then
        printf '%s\n' "$line" >> "$tmp"
        continue
      fi
      source=$(printf '%s\n' "$line" | sed -n 's/.*"source_path":"\([^"]*\)".*/\1/p')
      if [[ "$source" != *.xyz ]]; then
        printf '%s\n' "$line" >> "$tmp"
        continue
      fi
      energy=""
      log="${source%.xyz}.log"
      if [ -f "$log" ]; then
        while IFS= read -r line2 || [ -n "$line2" ]; do
          value=$(_jkcs_energy_from_line "$line2")
          if [ -n "$value" ]; then
            energy=$value
            break
          fi
        done < "$log"
      fi
      if [ -z "$energy" ] && [ -f "$source" ]; then
        comment=$(sed -n '2p' "$source")
        energy=$(_jkcs_energy_from_line "$comment")
      fi
      if [ -z "$energy" ]; then
        printf '%s\n' "$line" >> "$tmp"
        continue
      fi
      if [[ "$line" == *'}' ]]; then
        printf '%s,"electronic_energy":%s}\n' "${line%?}" "$energy" >> "$tmp"
        changed=$((changed + 1))
      else
        printf '%s\n' "$line" >> "$tmp"
      fi
    done < "$db"
    if [ "$changed" -gt 0 ]; then
      mode=$(stat -c '%a' "$db" 2>/dev/null || true)
      [ -z "$mode" ] || chmod "$mode" "$tmp"
      mv "$tmp" "$db"
      echo "$db: added electronic_energy to $changed record(s)" >&2
    else
      rm -f "$tmp"
    fi
  done
}
# <<< CJKQC patcher <<<
BLOCK
    my $placeholder = '__CJKQC_BIN__';
    my $placeholder_position = index($block, $placeholder);
    if ($placeholder_position >= 0) {
        $block = substr($block, 0, $placeholder_position)
            . $CJKQC_BIN
            . substr($block, $placeholder_position + length($placeholder));
    }
    my $text = slurp($path);
    my $begin = index($text, $start);
    my $finish = index($text, $end);
    if ($begin >= 0 && $finish > $begin) {
        $finish += length($end);
        my $prefix = substr($text, 0, $begin);
        my $after = substr($text, $finish);
        $after =~ s/^\n+//;
        my $new_text = $prefix . $block . $after;
        if ($new_text eq $text) {
            print "  skip   CJKQC helper block (already applied)\n";
            return;
        }
        spew($path, $new_text);
        print $DRY_RUN ? "  would  refresh CJKQC helper block\n"
                       : "  patch  refresh CJKQC helper block\n";
        return;
    }

    my $anchor = 'function program_JKQCpickle {';
    my $position = index($text, $anchor);
    my $new_text;
    if ($position >= 0) {
        $new_text = substr($text, 0, $position) . $block . substr($text, $position);
    } else {
        my $trimmed = $text;
        $trimmed =~ s/\n*\z//;
        $new_text = $trimmed . "\n\n" . $block;
    }
    spew($path, $new_text);
    print $DRY_RUN ? "  would  add CJKQC helper block\n"
                   : "  patch  add CJKQC helper block\n";
}

print "Patching JKCS2.1 in $JKCS_DIR\n";
print "  user setup: $USERSETUP\n";
print "  CJKQC:      $CJKQC_BIN\n";
print "  mode:       dry run\n" if $DRY_RUN;

install_block($USERSETUP);

my $old = <<'OLD';
    JKQC *.xyz -out collection${base}.pkl >/dev/null
    cp collection${base}.pkl $DIR/ 2>/dev/null
OLD
my $new = <<'NEW';
    program_CJKQC *.xyz --out collection${base}.jsonl --fout >/dev/null
    program_CJKQC_energy collection${base}.jsonl
    cp collection${base}.jsonl $DIR/ 2>/dev/null
NEW
patch($USERSETUP, $old, $new, 'CREST collection -> collection*.jsonl');

$old = 'addexploded=" -cut rg 50"';
$new = 'addexploded=" --cut rg 50"';
patch($EXPLORE, $old, $new, 'ABCluster explode cutoff -> CJKQC spelling');

$old = <<'OLD';
program_JKQCpickle -folder ./ -collect xyz -noex $addexploded -out collectionABC.pkl
OLD
$new = <<'NEW';
program_CJKQC *.xyz $addexploded --out collectionABC.jsonl --fout
program_CJKQC_energy collectionABC.jsonl
NEW
patch($EXPLORE, $old, $new, 'ABCluster collection -> collectionABC.jsonl');

$old = <<'OLD';
elif [ -e "collection${folderNEW}.pkl" ]
then
  JKecho 0 "File ${cfBLUE}collection${folderNEW}.pkl${cfDEF} already exists. [${cfRED}EXIT${cfDEF}]"
OLD
$new = <<'NEW';
elif [ -e "collection${folderNEW}.pkl" ] || [ -e "collection${folderNEW}.jsonl" ]
then
  JKecho 0 "File ${cfBLUE}collection${folderNEW}.pkl/jsonl${cfDEF} already exists. [${cfRED}EXIT${cfDEF}]"
NEW
patch($JKCS3, $old, $new, 'JKCS3_run collection existence check -> JSONL');

$old = 'program_JKQCpickle *.log -noex -out collection${folderOLD}.pkl; rm *.log; rm *.xyz;';
$new = 'program_CJKQC *.log --out collection${folderOLD}.jsonl --fout; rm *.log; rm *.xyz;';
patch($JKCS3, $old, $new, 'JKCS3_run old-folder recollect -> JSONL');

$old = <<'OLD';
          test=`ls $currentdirpath/$folderOLD/${sub}/${name}-LM/*.pkl 2> /dev/null | wc -w`
          if [ $test -eq 0 ]
          then
            JKecho 0 "No XYZ nor PKL file found in $currentdirpath/$folderOLD/${sub}/${name}-LM/."
OLD
$new = <<'NEW';
          test=`ls $currentdirpath/$folderOLD/${sub}/${name}-LM/*.pkl 2> /dev/null | wc -w`
          testjsonl=`ls $currentdirpath/$folderOLD/${sub}/${name}-LM/*.jsonl 2> /dev/null | wc -w`
          if [ $test -eq 0 ] && [ $testjsonl -eq 0 ]
          then
            JKecho 0 "No XYZ, PKL nor JSONL file found in $currentdirpath/$folderOLD/${sub}/${name}-LM/."
NEW
patch($JKCS3, $old, $new, 'JKCS3_run old-folder detects JSONL');

$old = <<'OLD';
program_JKQCpickle *.pkl -xyz -noex; cd $currentdirpath/$folderNEW;
OLD
$new = <<'NEW';
if ls *.pkl >/dev/null 2>&1; then program_JKQCpickle *.pkl -xyz -noex; else for db in *.jsonl; do program_PYTHON \"$toolspath/SCRIPTS/JKjsonl2xyz.py\" \"\$db\" . >/dev/null; done; fi; cd $currentdirpath/$folderNEW;
NEW
patch($JKCS3, $old, $new, 'JKCS3_run old-folder extracts JSONL to XYZ');

$old = <<'OLD';
    -collection[${cfBLUE}FOLDER${cfDEF}].pkl:
      crucial file that contains all data from the folder
OLD
$new = <<'NEW';
    -collection[${cfBLUE}FOLDER${cfDEF}].jsonl:
      crucial file that contains all data from the folder
NEW
patch($JKCS4, $old, $new, 'JKCS4 help -> collection*.jsonl');

$old = '      JKQC collection[${cfBLUE}FOLDER${cfDEF}].pkl -pXYZ -rg -el -dip -pol';
$new = '      CJKQC collection[${cfBLUE}FOLDER${cfDEF}].jsonl --path --rg --el --dip --pol';
patch($JKCS4, $old, $new, 'JKCS4 help example 1 -> CJKQC');

$old = '      JKQC collection[${cfBLUE}FOLDER${cfDEF}].pkl -ePKL -rg -el -g -lf ';
$new = '      CJKQC collection[${cfBLUE}FOLDER${cfDEF}].jsonl --path --rg --el --g --lf';
patch($JKCS4, $old, $new, 'JKCS4 help example 2 -> CJKQC');

$old = <<'OLD';
   -orca ............ if you collect results from ORCA runs
   OTHERS: -clean,-cleanfull,-forces,-eldisp,-forcedisp
OLD
$new = <<'NEW';
   -orca ............ if you collect results from ORCA runs
   -parallel N ...... worker count passed to CJKQC [def=1]
   OTHERS: -clean,-cleanfull,-forces,-eldisp,-forcedisp
NEW
patch($JKCS4, $old, $new, 'JKCS4 help -> -parallel');

$old = <<'OLD';
Qdisp_electronic_energy=0
Qdisp_forces=0

ADDdependency=""
OLD
$new = <<'NEW';
Qdisp_electronic_energy=0
Qdisp_forces=0
Qparallel=1 #CJKQC --parallel worker count

ADDdependency=""
NEW
patch($JKCS4, $old, $new, 'JKCS4 parallel default');

$old = <<'OLD';
    if [ "$last" == "-jpt" ];
    then
      last=""
      Qjpt=$iter
      continue
    fi
    # Forces
OLD
$new = <<'NEW';
    if [ "$last" == "-jpt" ];
    then
      last=""
      Qjpt=$iter
      continue
    fi
    # Parallel CJKQC workers
    if [[ "$iter" == "-parallel="* ]] || [[ "$iter" == "--parallel="* ]]
    then
      Qparallel="${iter#*=}"
      continue
    fi
    if [ "$iter" == "-parallel" ] || [ "$iter" == "--parallel" ]
    then
      last="-parallel"
      continue
    fi
    if [ "$last" == "-parallel" ]
    then
      Qparallel="$iter"
      last=""
      continue
    fi
    # Forces
NEW
patch($JKCS4, $old, $new, 'JKCS4 -parallel parsing');

$old = 'savepklfile="$collectionfile${method}.pkl"';
$new = 'savejsonlfile="$collectionfile${method}.jsonl"';
patch($JKCS4, $old, $new, 'JKCS4 output database -> collection*.jsonl');

patch($JKCS4, 'savepklfile', 'savejsonlfile', 'JKCS4 database variable rename', 1);

$old = '    rm $method/*.pkl $method/*/*.pkl $method/*/*/*.pkl 2>/dev/null';
$new = '    rm $method/*.pkl $method/*/*.pkl $method/*/*/*.pkl $method/*.jsonl $method/*/*.jsonl $method/*/*/*.jsonl 2>/dev/null';
patch($JKCS4, $old, $new, 'JKCS4 recollect removal includes JSONL');

$old = <<'OLD';
  PKLinsubfolders=`ls $method/*.pkl $method/*/*.pkl $method/*/*/*.pkl 2>/dev/null | xargs`
  if [ ! -z "$PKLinsubfolders" ]
OLD
$new = <<'NEW';
  JSONLinsubfolders=`ls $method/*.jsonl $method/*/*.jsonl $method/*/*/*.jsonl 2>/dev/null | xargs`
  if [ ! -z "$JSONLinsubfolders" ]
NEW
patch($JKCS4, $old, $new, 'JKCS4 subcollection lookup -> JSONL');

$old = 'if [ -z "$PKLinsubfolders" ]';
$new = 'if [ -z "$JSONLinsubfolders" ]';
patch($JKCS4, $old, $new, 'JKCS4 empty subcollection check -> JSONL');

$old = '      rm *.log *.pkl 2>/dev/null';
$new = '      rm *.log *.pkl *.jsonl 2>/dev/null';
patch($JKCS4, $old, $new, 'JKCS4 ABCluster cleanup includes JSONL');

$old = <<'OLD';
      echo "source ~/.JKCSusersetup.txt;" > run${scriptfilecommand}.sh
      echo "program_JKQCpickle $addarg -noex -folder ./ -out $savejsonlfile" >> run${scriptfilecommand}.sh
OLD
$new = <<'NEW';
      echo "source ~/.JKCSusersetup.txt;" > run${scriptfilecommand}.sh
      if [[ "$method" == *"ABC"* ]] && [[ "$method" != *"DFT"* ]]
      then
        echo "program_CJKQC \$(ls *.xyz 2>/dev/null) --parallel $Qparallel --out $savejsonlfile --fout; program_CJKQC_energy $savejsonlfile" >> run${scriptfilecommand}.sh
      else
        echo "inputs=\$(ls *.log *.out 2>/dev/null); if [ -z \"\$inputs\" ]; then inputs=\$(ls *.xyz 2>/dev/null); fi; program_CJKQC \$inputs --parallel $Qparallel --out $savejsonlfile --fout; program_CJKQC_energy $savejsonlfile" >> run${scriptfilecommand}.sh
      fi
NEW
patch($JKCS4, $old, $new, 'JKCS4 per-subfolder collection -> CJKQC');

$old = '    arguments=( "-ePKL" )';
$new = '    arguments=( "--path" )';
patch($JKCS4, $old, $new, 'JKCS4 default table columns -> CJKQC');

$old = <<'OLD';
  arguments+=( "-rg" )
  arguments+=( "-el" )
OLD
$new = <<'NEW';
  arguments+=( "--rg" )
  arguments+=( "--el" )
NEW
patch($JKCS4, $old, $new, 'JKCS4 table columns -> CJKQC flags');

$old = '    arguments_hcatch+=( "-g" )';
$new = '    arguments_hcatch+=( "--g" )';
patch($JKCS4, $old, $new, 'JKCS4 -g -> CJKQC --g');

$old = '    arguments_hcatch+=( "-g -v 0.996 -fc 100" )';
$new = '    arguments_hcatch+=( "--g --fscale 0.996 --fc 100" )';
patch($JKCS4, $old, $new, 'JKCS4 -gh -> CJKQC flags');

$old = '    arguments_hcatch+=( "$iter" )';
$new = '    arguments_hcatch+=( "--dip" )';
patch($JKCS4, $old, $new, 'JKCS4 -dip -> CJKQC --dip');

$old = <<'OLD';
else
  Qs=-1
  Qu=""
fi

### GOING THROUGH ALL SUBBFOLDERS
OLD
$new = <<'NEW';
else
  Qs=-1
  Qu=""
fi

if [ "$Qs" == "4" ]
then
  if [[ " ${arguments[*]} " == *" --dip "* ]]; then tableawk='{print $1, $4, $2, $3, $5}'; else tableawk='{print $1, $4, $2, $3}'; fi
elif [ "$Qs" == "3" ]
then
  if [[ " ${arguments[*]} " == *" --dip "* ]]; then tableawk='{print $1, $3, $2, $4}'; else tableawk='{print $1, $3, $2}'; fi
else
  tableawk='{print}'
fi

### GOING THROUGH ALL SUBBFOLDERS
NEW
patch($JKCS4, $old, $new, 'JKCS4 table reorder -> legacy JKQC column order');

patch($JKCS4, 'PKLinsubfolders', 'JSONLinsubfolders',
    'JKCS4 commandTODO variable rename', 1);
patch($JKCS4, '$method/c*.pkl $method/*/c*.pkl $method/*/*/c*.pkl',
    '$method/c*.jsonl $method/*/c*.jsonl $method/*/*/c*.jsonl',
    'JKCS4 commandTODO globs -> JSONL', 1);

$old = <<'OLD';
program_JKQCpickle \$JSONLinsubfolders -out $savejsonlfile >> output; program_JKQCpickle  $savejsonlfile ${arguments[@]} >  $savefile;
OLD
$new = <<'NEW';
if [ -n \"\$JSONLinsubfolders\" ]; then program_CJKQC \$JSONLinsubfolders --parallel $Qparallel --out $savejsonlfile --fout >> output; fi; program_CJKQC_energy $savejsonlfile >> output 2>&1; program_CJKQC $savejsonlfile --parallel $Qparallel ${arguments[@]} | tail -n +2 | awk '$tableawk' > $savefile;
NEW
patch($JKCS4, $old, $new, 'JKCS4 merge + table generation -> CJKQC', 1);

$old = <<'OLD';
  ending=`echo $savefile | rev | cut -c-4 | rev`
  if [ "$ending" == ".pkl" ]
  then
    savefile=$(basename $savefile .pkl).txt
    Qpickle=1
  else
    Qpickle=0
  fi
OLD
$new = <<'NEW';
  case "$savefile" in
    *.pkl|*.jsonl) Qpickle=1 ;;
    *) Qpickle=0 ;;
  esac
  if [ $Qpickle -eq 1 ]
  then
    savefile=`echo "$savefile" | sed 's/\.pkl$/.txt/; s/\.jsonl$/.txt/'`
  fi
NEW
patch($JKCOLLECT, $old, $new, 'JKcollect UNIQUENESS recognizes JSONL');

$old = <<'OLD';
  ending=`echo $savefile | rev | cut -c-4 | rev`
  if [ "$ending" != ".pkl" ]
  then
OLD
$new = <<'NEW';
  ending=`echo $savefile | rev | cut -c-4 | rev`
  if [ "$ending" != ".pkl" ] && [[ "$savefile" != *.jsonl ]]
  then
NEW
patch($JKCOLLECT, $old, $new, 'JKcollect CONTROL skips concatenation for JSONL');

$old = <<'OLD';
  else
    o=`cat $resultfile | sed 's/\/\:EXTRACT\:\// /g' | awk '{print $2}' | xargs | sed "s/ /,/g"`;
    JKQC $(basename $savefile .txt).pkl -extract $o -movie -noname
    mv movie.xyz $movie
  fi
OLD
$new = <<'NEW';
  else
    db="${savefile%.*}.jsonl"
    if [ -e "$db" ]; then
      o=`cat $resultfile | sed 's/^[^ ]*\/:EXTRACT:\/\([^ ]*\).*/\1/; s/^\([^ ]*\) .*/\1/' | sed 's#.*/##' | sed 's/\.[^.]*$//' | xargs`
      extractargs=""
      for name in $o
      do
        extractargs+=" --extract $name"
      done
      program_CJKQC "$db" $extractargs --noname --movie --fout
    else
      db="${savefile%.*}.pkl"
      o=`cat $resultfile | sed 's/\/\:EXTRACT\:\// /g' | awk '{print $2}' | xargs | sed "s/ /,/g"`
      JKQC "$db" -extract $o -movie -noname
    fi
    mv movie.xyz $movie
  fi
NEW
patch($JKCOLLECT, $old, $new, 'JKcollect movie export -> CJKQC');

print "Done." . ($DRY_RUN ? " No files were written (dry run)." : "") . "\n";
PERL
