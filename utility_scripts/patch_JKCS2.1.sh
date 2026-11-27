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
#   --no-backup        Do not create .*.cjkqc-patch.bak backups
#   -h, --help         Show this help
#
# What it does:
#   * adds CJKQC_BIN and program_CJKQC to the user setup file
#   * ABCluster collection (JKCS2_explore) writes collectionABC.jsonl via CJKQC
#   * ABCluster collection runs in the job scratch directory, not the submit dir
#   * CREST collection (program_CREST in the user setup) writes JSONL via CJKQC
#   * a failed CREST run leaves no partial collection behind
#   * JKCS3_run checks/writes JSONL collections where it used to write PKL
#   * JKCS3_run extracts JSONL structures on the compute nodes, not the frontend
#   * hot per-file loops use shell arithmetic instead of forking bc
#   * JKCS4_collect and JKcollect use CJKQC + JSONL instead of the bundled JKQC
#   * patches a helper function that carries ABC/CREST energies into the records
#
# Backups are hidden (.<name>.cjkqc-patch.bak) so that script globs such as
# JKCSx/JKCS2* do not mistake them for the real next-step script.
#
# The patch is idempotent; running it again only refreshes the CJKQC path.

set -euo pipefail

usage() {
  sed -n '3,32p' "$0" | sed 's/^# \{0,1\}//'
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
use File::Basename qw(basename dirname);
use File::Copy qw(copy);

my $JKCS_DIR = $ENV{JKCS_DIR};
my $USERSETUP = $ENV{USERSETUP};
my $CJKQC_BIN = $ENV{CJKQC_BIN};
my $DRY_RUN = $ENV{DRY_RUN} eq '1';
my $BACKUP = $ENV{BACKUP} eq '1';

my $JKCS1 = "$JKCS_DIR/JKCSx/JKCS1_prepare";
my $JKCS3 = "$JKCS_DIR/JKCSx/JKCS3_run";
my $JKCS4 = "$JKCS_DIR/JKCSx/JKCS4_collect";
my $EXPLORE = "$JKCS_DIR/JKCSx/JKCS2_explore";
my $JKCOLLECT = "$JKCS_DIR/TOOLS/SCRIPTS/JKcollect";
my $JSONL_HELPER = "$JKCS_DIR/TOOLS/SCRIPTS/JKjsonl_records.py";

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
    return unless -e $path;
    my $backup = dirname($path) . '/.' . basename($path) . '.cjkqc-patch.bak';
    my $old_backup = "$path.cjkqc-patch.bak";
    if (!-e $backup && -e $old_backup) {
        rename($old_backup, $backup)
            or die "error: cannot move backup $old_backup to $backup: $!\n";
    }
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

sub patch_variants {
    my ($path, $variants, $new, $label, $all) = @_;
    chomp $new;
    my $text = slurp($path);
    for my $old (@$variants) {
        chomp $old;
        if (index($text, $old) >= 0) {
            patch($path, $old, $new, $label, $all);
            return;
        }
    }
    if (index($text, $new) >= 0) {
        print "  skip   $label (already applied)\n";
    } else {
        print "  skip   $label (pattern not present)\n";
    }
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

sub install_jsonl_helper {
    my ($path) = @_;
    my $content = <<'PY';
#!/usr/bin/env python3
"""List or extract records from a CJKQC JSON Lines database.

Usage:
  JKjsonl_records.py list DATABASE.jsonl
  JKjsonl_records.py extract DATABASE.jsonl OUTPUT_DIRECTORY LINE [LINE...]

`list` prints LINE<TAB>NAME for every record that has a usable geometry.
`extract` writes only the requested records as NAME.xyz into OUTPUT_DIRECTORY.
"""

import json
import math
import os
import re
import sys


def fail(message):
  print("error: " + message, file=sys.stderr)
  sys.exit(1)


def safe_name(name, line_number):
  if not isinstance(name, str) or not name.strip():
    return "record_{}".format(line_number)
  name = os.path.basename(name.strip())
  name = re.sub(r"[^A-Za-z0-9._-]+", "_", name)
  name = name.strip("._-")
  if not name:
    return "record_{}".format(line_number)
  return name


def record_name(record, line_number):
  name = record.get("name")
  if isinstance(name, str) and name.strip():
    return safe_name(name, line_number)
  source_path = record.get("source_path")
  if isinstance(source_path, str) and source_path.strip():
    stem = os.path.splitext(os.path.basename(source_path.strip()))[0]
    if stem:
      return safe_name(stem, line_number)
  return safe_name(None, line_number)


def extract_geometry(record):
  geometry = record.get("geometry")
  if not isinstance(geometry, dict):
    return None
  symbols = geometry.get("symbols")
  coordinates = geometry.get("coordinates")
  if not isinstance(symbols, list) or not isinstance(coordinates, list):
    return None
  if not symbols or len(coordinates) != 3 * len(symbols):
    return None
  for symbol in symbols:
    if not isinstance(symbol, str) or not symbol:
      return None
  values = []
  for coordinate in coordinates:
    if isinstance(coordinate, bool) or not isinstance(coordinate, (int, float)):
      return None
    coordinate = float(coordinate)
    if not math.isfinite(coordinate):
      return None
    values.append(coordinate)
  return symbols, values


def write_xyz(path, name, geometry):
  symbols, coordinates = geometry
  try:
    with open(path, "w", encoding="utf-8") as xyz:
      xyz.write("{}\n".format(len(symbols)))
      xyz.write("{}\n".format(name))
      for atom in range(len(symbols)):
        xyz.write("{} {:.10f} {:.10f} {:.10f}\n".format(
          symbols[atom],
          coordinates[3 * atom],
          coordinates[3 * atom + 1],
          coordinates[3 * atom + 2]))
  except OSError as error:
    fail("cannot write {}: {}".format(path, error))


def parse_record(line, database, line_number, warn):
  line = line.strip()
  if not line:
    return None
  try:
    record = json.loads(line)
  except json.JSONDecodeError as error:
    if warn:
      print("warning: {}:{}: cannot parse JSON ({}); skipping".format(
        database, line_number, error), file=sys.stderr)
    return None
  if not isinstance(record, dict):
    if warn:
      print("warning: {}:{}: record is not a JSON object; skipping".format(
        database, line_number), file=sys.stderr)
    return None
  return record


def command_list(database):
  names = {}
  count = 0
  try:
    handle = open(database, "r", encoding="utf-8")
  except OSError as error:
    fail("cannot read {}: {}".format(database, error))
  with handle:
    for line_number, line in enumerate(handle, 1):
      record = parse_record(line, database, line_number, True)
      if record is None:
        continue
      geometry = extract_geometry(record)
      if geometry is None:
        print("warning: {}:{}: no usable geometry; skipping".format(
          database, line_number), file=sys.stderr)
        continue
      name = record_name(record, line_number)
      if name in names:
        fail("duplicate structure name '{}' at lines {} and {}".format(
          name, names[name], line_number))
      names[name] = line_number
      print("{}\t{}".format(line_number, name))
      count += 1
  if count == 0:
    fail("no usable structures found in {}".format(database))


def command_extract(database, outdir, requested):
  wanted = set()
  for value in requested:
    try:
      wanted.add(int(value))
    except ValueError:
      fail("invalid line number: {}".format(value))
  if not wanted:
    fail("no line numbers requested")
  try:
    os.makedirs(outdir, exist_ok=True)
  except OSError as error:
    fail("cannot create output directory {}: {}".format(outdir, error))
  written = set()
  try:
    handle = open(database, "r", encoding="utf-8")
  except OSError as error:
    fail("cannot read {}: {}".format(database, error))
  with handle:
    for line_number, line in enumerate(handle, 1):
      if line_number not in wanted:
        continue
      record = parse_record(line, database, line_number, True)
      geometry = None if record is None else extract_geometry(record)
      if geometry is None:
        fail("no usable structure at line {}".format(line_number))
      name = record_name(record, line_number)
      path = os.path.abspath(os.path.join(outdir, name + ".xyz"))
      write_xyz(path, name, geometry)
      print(path)
      written.add(line_number)
      if written == wanted:
        break
  missing = wanted - written
  if missing:
    fail("no usable structure at line(s): {}".format(
      " ".join(str(number) for number in sorted(missing))))


def main(argv):
  if len(argv) < 3 or argv[1] in ("-h", "--help"):
    print("Usage: JKjsonl_records.py list DATABASE.jsonl\n"
          "       JKjsonl_records.py extract DATABASE.jsonl OUTPUT_DIRECTORY LINE [LINE...]",
          file=sys.stderr)
    return 2
  database = argv[2]
  if not os.path.isfile(database):
    fail("database does not exist: {}".format(database))
  if argv[1] == "list":
    if len(argv) != 3:
      fail("list takes exactly one database")
    command_list(database)
    return 0
  if argv[1] == "extract":
    if len(argv) < 5:
      fail("extract needs an output directory and at least one line number")
    command_extract(database, argv[3], argv[4:])
    return 0
  fail("unknown command: {}".format(argv[1]))


if __name__ == "__main__":
  sys.exit(main(sys.argv))
PY
    if (-f $path) {
        my $text = slurp($path);
        if ($text eq $content) {
            print "  skip   JSONL records helper (already applied)\n";
            return;
        }
    }
    spew($path, $content);
    print $DRY_RUN ? "  would  install JSONL records helper\n"
                   : "  patch  install JSONL records helper\n";
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

$old = <<'OLD';
  ${PATH_CREST}/crest calc.xyz $method $args &> $DIR/$output
OLD
$new = <<'NEW';
  # A crashed CREST run can still leave partial conformer files behind; do
  # not let those become a collection that the next JKCS step would read.
  if ${PATH_CREST}/crest calc.xyz $method $args &> $DIR/$output
  then
    :
  else
    crest_status=$?
    echo "CREST failed (status $crest_status); no collection written. See $DIR/$output." >&2
    cd "$DIR"
    rm -rf "$CALC_NAME"
    return $crest_status
  fi
NEW
patch($USERSETUP, $old, $new, 'CREST failure leaves no partial collection');

$old = 'addexploded=" -cut rg 50"';
$new = 'addexploded=" --cut rg 50"';
patch($EXPLORE, $old, $new, 'ABCluster explode cutoff -> CJKQC spelling');

my $collection_scratch = <<'NEW';
      echo """cd $name-LM
source ~/.JKCSusersetup.txt$loc
collectdir=\"\${WRKDIR:-.}/TMP/ABCCOLLECT_\${SLURM_JOBID:-\$\$}_${const_iter}\"
mkdir -p \"\$collectdir\"
cp *.xyz \"\$collectdir\"/ 2>/dev/null
(
  cd \"\$collectdir\"
  program_CJKQC *.xyz $addexploded --out collectionABC.jsonl --fout
  program_CJKQC_energy collectionABC.jsonl
)
cp \"\$collectdir/collectionABC.jsonl\" .
rm -rf \"\$collectdir\"
rm -f *.xyz *.log
cd ..""" >> rename.sh
NEW

my $collection_old_patched = <<'OLD';
      echo """cd $name-LM
source ~/.JKCSusersetup.txt$loc
for i in *.xyz
do
  base=\$(basename \$i .xyz)
  test=\`grep -c \"Energy:\" \$i\`
  if [ \$test -gt 0 ]
  then
    energy=\`grep \"Energy:\" \$i | awk '{print \$2}'\`
  else
    energy=\`head -n 2 \$i | tail -n 1\`
  fi
  echo \"ABC\" > \${base}.log
  echo \"ABC energy: \$energy\" >> \${base}.log
done
program_CJKQC *.xyz $addexploded --out collectionABC.jsonl --fout
program_CJKQC_energy collectionABC.jsonl
rm *.xyz *.log
cd ..""" >> rename.sh
OLD
my $collection_old_pristine = <<'OLD';
      echo """cd $name-LM
source ~/.JKCSusersetup.txt$loc
for i in *.xyz
do
  base=\$(basename \$i .xyz)
  test=\`grep -c \"Energy:\" \$i\`
  if [ \$test -gt 0 ]
  then
    energy=\`grep \"Energy:\" \$i | awk '{print \$2}'\`
  else
    energy=\`head -n 2 \$i | tail -n 1\`
  fi
  echo \"ABC\" > \${base}.log
  echo \"ABC energy: \$energy\" >> \${base}.log
done
program_JKQCpickle -folder ./ -collect xyz -noex $addexploded -out collectionABC.pkl
rm *.xyz *.log
cd ..""" >> rename.sh
OLD
patch_variants($EXPLORE, [$collection_old_patched, $collection_old_pristine],
    $collection_scratch, 'ABCluster collection in scratch -> collectionABC.jsonl');

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

my $merge_old = <<'OLD';
program_JKQCpickle \$JSONLinsubfolders -out $savejsonlfile >> output; program_JKQCpickle  $savejsonlfile ${arguments[@]} >  $savefile;
OLD
my $merge_old_patched = <<'OLD';
if [ -n \"\$JSONLinsubfolders\" ]; then program_CJKQC \$JSONLinsubfolders --parallel $Qparallel --out $savejsonlfile --fout >> output; fi; program_CJKQC_energy $savejsonlfile >> output 2>&1; program_CJKQC $savejsonlfile --parallel $Qparallel ${arguments[@]} | tail -n +2 | awk '$tableawk' > $savefile;
OLD
my $merge_new = <<'NEW';
if [ -n \"\$JSONLinsubfolders\" ]; then program_CJKQC \$JSONLinsubfolders --parallel $Qparallel --out $savejsonlfile --fout >> output; fi; program_CJKQC_energy $savejsonlfile >> output 2>&1; if [ $Qonlycollect -eq 0 ]; then program_CJKQC $savejsonlfile --parallel $Qparallel ${arguments[@]} | tail -n +2 | awk '$tableawk' > $savefile; fi;
NEW
patch_variants($JKCS4, [$merge_old, $merge_old_patched], $merge_new,
    'JKCS4 merge -> CJKQC JSONL (text table only without -oc)', 1);

$old = <<'OLD';
if [ -e $savefile ]; then rm $savefile; fi
touch $savefile
if [ -e $resultfile ]; then rm $resultfile;fi
if [ -e $movie ]; then rm $movie;fi
JKecho 2 "Output files checked (${cfYELLOW}$savefile${cfDEF},${cfYELLOW}$resultfile${cfDEF},${cfYELLOW}$movie${cfDEF})"
OLD
$new = <<'NEW';
if [ $Qonlycollect -eq 0 ]
then
  if [ -e $savefile ]; then rm $savefile; fi
  touch $savefile
  if [ -e $resultfile ]; then rm $resultfile;fi
  if [ -e $movie ]; then rm $movie;fi
fi
JKecho 2 "Output files checked (${cfYELLOW}$savejsonlfile${cfDEF},${cfYELLOW}$resultfile${cfDEF},${cfYELLOW}$movie${cfDEF})"
NEW
patch($JKCS4, $old, $new, 'JKCS4 onlycollect skips text database');

$old = <<'OLD';
if [ "$Qperform" != 2 ]
then
  JKecho 3 "Command ${cfMAGENTA}$commandTODO${cfDEF}"
  echo  "$SC_command '$commandTODO'" >> output
  JKecho 3 "Submitting CONTROL command"
  if [ $SCtasks -eq 0 ]
  then
    SC_command_dep=$SC_command
  else
    job_id=`echo $job | awk '{print $4}'`
    SC_command_dep=`echo $SC_command | sed "s/sbatch/sbatch --dependency=afterany:$job_id/"`
  fi
  echo  "$SC_command_dep '$commandTODO'" > .runC
  job=$(/bin/bash .runC)
OLD
$new = <<'NEW';
if [ "$Qperform" != 2 ]
then
  printf '%s\n' "$commandTODO" > .runC_command
  JKecho 3 "Command ${cfMAGENTA}$commandTODO${cfDEF}"
  echo  "$SC_command sh .runC_command" >> output
  JKecho 3 "Submitting CONTROL command"
  if [ $SCtasks -eq 0 ]
  then
    SC_command_dep=$SC_command
  else
    job_id=`echo $job | awk '{print $4}'`
    SC_command_dep=`echo $SC_command | sed "s/sbatch/sbatch --dependency=afterany:$job_id/"`
  fi
  echo  "$SC_command_dep sh .runC_command" > .runC
  job=$(/bin/bash .runC)
NEW
patch($JKCS4, $old, $new, 'JKCS4 control job quoting -> command file');

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

if (index(slurp($JKCS3), 'JKjsonl2xyz.py') >= 0
    || index(slurp($JKCS3), 'JKjsonl_records.py') >= 0) {
    install_jsonl_helper($JSONL_HELPER);

    $old = <<'OLD';
  if [ "$(echo ${resultsaddition} | rev | cut -c-6 | rev)" == ".jsonl" ]
  then
    jsonlxyzdir="$currentdirpath/.JKCS3_jsonl_$(basename "$resultsaddition" .jsonl).$$"
    trap 'rm -rf "$jsonlxyzdir"; rm -f "$currentdirpath/toexecute.txt"' EXIT
    if ! program_PYTHON "$toolspath/SCRIPTS/JKjsonl2xyz.py" "$resultsaddition" "$jsonlxyzdir" > toexecute.txt
    then
      JKecho 0 "Could not extract XYZ structures from ${cfYELLOW}${resultsaddition}${cfDEF}. [${cfRED}EXIT${cfDEF}]"
      exit 1
    fi
    resultsaddition=toexecute.txt
    Qjsonl=1
  fi
OLD
    $new = <<'NEW';
  if [ "$(echo ${resultsaddition} | rev | cut -c-6 | rev)" == ".jsonl" ]
  then
    jsonldb="$( cd "$(dirname "${resultsaddition}")" ; pwd -P )"/$(basename "$resultsaddition")
    trap 'rm -f "$currentdirpath/toexecute.txt"' EXIT
    if ! program_PYTHON "$toolspath/SCRIPTS/JKjsonl_records.py" list "$jsonldb" > toexecute.txt
    then
      JKecho 0 "Could not read XYZ structures from ${cfYELLOW}${resultsaddition}${cfDEF}. [${cfRED}EXIT${cfDEF}]"
      exit 1
    fi
    declare -A jsonlnames=()
    while IFS=$'\t' read -r jsonlline jsonlname
    do
      jsonlnames[$jsonlline]=$jsonlname
    done < toexecute.txt
    resultsaddition=toexecute.txt
    Qjsonl=1
  fi
NEW
    patch($JKCS3, $old, $new, 'JKCS3_run JSONL input lists records only');

    $old = <<'OLD';
    if [ "$Qjsonl" == "1" ]
    then
      cp "$file" "$currentdirpath/$folderNEW/${name}-LM$ADDc/" || {
        JKecho 0 "Could not copy ${cfYELLOW}$file${cfDEF} into ${cfBLUE}$folderNEW/${name}-LM$ADDc${cfDEF}. [${cfRED}EXIT${cfDEF}]"
        exit 1
      }
      file="$(basename "$file")"
    fi
OLD
    $new = <<'NEW';
    if [ "$Qjsonl" == "1" ]
    then
      jsonlline=$file
      file=${jsonlnames[$jsonlline]}
      if [ -z "$file" ]
      then
        JKecho 0 "Could not resolve JSONL record at line ${cfYELLOW}$jsonlline${cfDEF}. [${cfRED}EXIT${cfDEF}]"
        exit 1
      fi
      jsonlout="$currentdirpath/$folderNEW/${name}-LM$ADDc"
      file="$file.xyz"
    fi
NEW
    patch($JKCS3, $old, $new, 'JKCS3_run JSONL input no frontend copy');

    $old = <<'OLD';
    #command="source ~/.JKCSusersetup.txt;\n program_$program $currentdirapth/$folderOLD/${name}-LM/$file${Qcpall}"
    #printf "$command" > run${scriptfilecommand}.sh
    commandTODO+="cd $currentdirpath/$folderNEW/${name}-LM$ADDc/; program_$program $file${addbase}${Qcpall};"
OLD
    $new = <<'NEW';
    #command="source ~/.JKCSusersetup.txt;\n program_$program $currentdirapth/$folderOLD/${name}-LM/$file${Qcpall}"
    #printf "$command" > run${scriptfilecommand}.sh
    if [ "$Qjsonl" == "1" ]
    then
      commandTODO+="if [ ! -e \"$jsonlout/$file\" ]; then program_PYTHON \"$toolspath/SCRIPTS/JKjsonl_records.py\" extract \"$jsonldb\" \"$jsonlout\" $jsonlline >/dev/null || exit 1; fi;"
    fi
    commandTODO+="cd $currentdirpath/$folderNEW/${name}-LM$ADDc/; program_$program $file${addbase}${Qcpall};"
NEW
    patch($JKCS3, $old, $new, 'JKCS3_run JSONL extraction moved into jobs');

    $old = <<'OLD';
  if [ "$Qjsonl" == "1" ]
  then
    rm -rf "$jsonlxyzdir"
  fi
OLD
    $new = <<'NEW';
  if [ "$Qjsonl" == "1" ] && [ -n "$jsonlxyzdir" ]
  then
    rm -rf "$jsonlxyzdir"
  fi
NEW
    patch($JKCS3, $old, $new, 'JKCS3_run JSONL temporary cleanup guard');
} else {
    print "  skip   JKCS3_run JSONL extraction (JSONL input support not found)\n";
}

patch($JKCS3, 'count=`echo $count+1 | bc`', 'count=$((count + 1))',
    'JKCS3_run per-file counter uses shell arithmetic');
patch($JKCS3, 'enum=`echo $enum+1 | bc`', 'enum=$((enum + 1))',
    'JKCS3_run folder counter uses shell arithmetic', 1);
patch($JKCS3, 'Qjpt_iter=`echo $Qjpt_iter + 1 |bc`', 'Qjpt_iter=$((Qjpt_iter + 1))',
    'JKCS3_run tasks-per-job counter uses shell arithmetic', 1);
patch($JKCS3, 'const_iter=`echo $const_iter+1 |bc`', 'const_iter=$((const_iter + 1))',
    'JKCS3_run job counter uses shell arithmetic', 1);
patch($JKCS3, 'jobscount=`echo $jobscount+1|bc`', 'jobscount=$((jobscount + 1))',
    'JKCS3_run job total uses shell arithmetic', 1);
patch($JKCS3, 'fi=\`echo \$fi+1|bc\`', 'fi=\$((fi+1))',
    'JKCS3_run old-folder file counter uses shell arithmetic', 1);

patch($EXPLORE, 'Qjpt_iter=`echo $Qjpt_iter + 1 |bc`',
    'Qjpt_iter=$((Qjpt_iter + 1))',
    'JKCS2_explore tasks-per-job counter uses shell arithmetic');
patch($EXPLORE, 'const_iter=`echo $const_iter+1 |bc`',
    'const_iter=$((const_iter + 1))',
    'JKCS2_explore job counter uses shell arithmetic');

patch($JKCS4, 'Qjpt_iter=`echo $Qjpt_iter + 1 |bc`',
    'Qjpt_iter=$((Qjpt_iter + 1))',
    'JKCS4_collect subfolder counter uses shell arithmetic');

for my $performer ("$JKCS_DIR/TOOLS/SCRIPTS/JKperform.sh",
                  "$JKCS_DIR/TOOLS/SCRIPTS/JKperform2.sh") {
    next unless -f $performer;
    my $name = basename($performer);
    patch($performer, 'local linenumber=`echo $linenumber+1 | bc`',
        'local linenumber=$((linenumber + 1))',
        "$name command-line counter uses shell arithmetic");
    patch($performer, 'doingcount=`echo $doingcount+1 | bc`',
        'doingcount=$((doingcount + 1))',
        "$name doing counter uses shell arithmetic");
}

if (-f $JKCS1) {
    $old = 'nextscript=`ls $scriptpath/JKCS2*`';
    $new = 'nextscript=`ls $scriptpath/JKCS2* 2>/dev/null | grep -v "\.cjkqc-patch\.bak$" | head -n 1`';
    patch($JKCS1, $old, $new, 'JKCS1 next-step ignores patch backups');
} else {
    print "  skip   JKCS1 next-step (JKCS1_prepare not found)\n";
}

print "Done." . ($DRY_RUN ? " No files were written (dry run)." : "") . "\n";
PERL
