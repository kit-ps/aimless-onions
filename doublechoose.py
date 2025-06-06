import itertools
import math
import re
from functools import reduce

bw_re = re.compile("^w .*Bandwidth=(\\d+)")

def load_from_consensus(path):
    """Loads a list of relay probabilities from the given consensus file."""
    relays = []
    with open(path, "r") as consfile:
        for line in consfile:
            if match := bw_re.search(line):
                weight = int(match.group(1))
                relays.append(weight)

    return relays


def probability(probs: list[float], path_length: int) -> float:
    """Returns the probability to double-choose a node.

    The probabilities of the single relays are given in ``probs``, they will be
    normalized.

    The ``path_length`` gives the length of the path. A longer path has a
    higher change to have duplicated nodes.
    """
    sum_probs = sum(probs)
    probs = [prob / sum_probs for prob in probs]
    prob = 0.0

    if path_length < 2:
        return 0.0

    if len(probs) < path_length:
        return 1.0

    for i, relay in enumerate(probs):
        other_relays = probs[:i] + probs[i + 1:]
        # We compute the probability that *this specific* relay is chosen (at
        # least) twice
        for k in range(2, path_length + 1):
            # We cannot use (1 - relay)**(path_length - k) as that could
            # contain other duplicates as well, in which case this
            # "combination" would be counted multiple times.
            unique_remainder = 0.0
            for path in itertools.combinations(other_relays, path_length - k):
                unique_remainder += math.factorial(path_length - k) * reduce(lambda a, b: a * b, path, 1.0)
            prob += math.comb(path_length, k) * relay**k * unique_remainder

    return prob
