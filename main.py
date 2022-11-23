from config import config
import argparse


def start_ui(mode=None):
    import ui
    ui.main(mode=mode)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "-m",
        "--mode",
        help="指定使用的浏览器",
    )
    args = parser.parse_args()
    start_ui(args.mode)
