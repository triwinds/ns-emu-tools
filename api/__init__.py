import os
import pkgutil
from importlib import import_module

__all__ = list(module for _, module, _ in pkgutil.iter_modules([os.path.dirname(__file__)]))

for m in __all__:
    import_module(f'.{m}', 'api')
