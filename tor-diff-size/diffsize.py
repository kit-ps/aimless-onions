import datetime
import os
from pathlib import Path
from subprocess import check_call

outdir = Path("consensuses")

outdir.mkdir(exist_ok=True)

def consensus_url(date, flavor=None):
    if flavor is None:
        fname = date.strftime("%Y-%m-%d-%H-%M-%S-consensus")
        url = f"https://collector.torproject.org/recent/relay-descriptors/consensuses/{fname}"
    else:
        fname = date.strftime("%Y-%m-%d-%H-%M-%S-consensus-microdesc")
        url = f"https://collector.torproject.org/recent/relay-descriptors/microdescs/consensus-microdesc/{fname}"
    return url

# Download the 10 last conensuses, we expect those to have the diffs around
# Skip the last hour to ensure the consensus has been uploaded already
now = datetime.datetime.now(datetime.UTC)
last_consensus = now.replace(minute=0, second=0, microsecond=0)
consensi = [last_consensus - datetime.timedelta(hours=h) for h in range(1, 11)]
urls = [consensus_url(c, flavor=None) for c in consensi]

check_call(["curl", "--output-dir", str(outdir), "--remote-name-all"] + urls)

# We need to strip off the @type line at the top
for consensus in outdir.iterdir():
    content = "\n".join(consensus.read_text().split("\n", 1)[1:])
    consensus.write_text(content)

# For each of the consensues, retrieve the diff using our rust tool
check_call(["cargo", "run", "--"] + list(map(str, outdir.iterdir())))
