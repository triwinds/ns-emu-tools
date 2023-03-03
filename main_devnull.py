import os
import sys

f = open(os.devnull, 'w')
sys.stdout = f
sys.stderr = f


if __name__ == '__main__':
    from main import main
    main()
