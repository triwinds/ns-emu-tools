import subprocess
import logging


logger = logging.getLogger(__name__)


def get_gpu_info():
    # https://github.com/SummaLabs/DLS/blob/master/app/backend/env/hardware.py
    gpu_info = []
    try:
        command = "nvidia-smi --query-gpu=index,name,uuid,memory.total,memory.free," \
                  "memory.used,count,utilization.gpu,utilization.memory --format=csv"
        output = execute_command(command)
        lines = output.splitlines()
        lines.pop(0)
        for line in lines:
            tokens = line.split(", ")
            if len(tokens) > 6:
                gpu_info.append({'id': tokens[0], 'name': tokens[1], 'mem': tokens[3], 'cores': tokens[6],
                                 'mem_free': tokens[4], 'mem_used': tokens[5],
                                 'util_gpu': tokens[7], 'util_mem': tokens[8]})
    except OSError:
        logger.info("GPU device is not available")

    return gpu_info


def execute_command(cmd):
    process = subprocess.Popen(cmd, stdout=subprocess.PIPE)
    return process.communicate()[0].decode()


def get_cpu_info():
    import platform
    return platform.processor()


def get_win32_cpu_info():
    # https://github.com/pydata/numexpr/blob/master/numexpr/cpuinfo.py
    import re
    import sys
    from utils.string_util import auto_decode
    pkey = r"HARDWARE\DESCRIPTION\System\CentralProcessor"
    try:
        import _winreg
    except ImportError:  # Python 3
        import winreg as _winreg
    info = []
    try:
        # XXX: Bad style to use so long `try:...except:...`. Fix it!

        prgx = re.compile(r"family\s+(?P<FML>\d+)\s+model\s+(?P<MDL>\d+)"
                          r"\s+stepping\s+(?P<STP>\d+)", re.IGNORECASE)
        chnd = _winreg.OpenKey(_winreg.HKEY_LOCAL_MACHINE, pkey)
        pnum = 0
        while 1:
            try:
                proc = _winreg.EnumKey(chnd, pnum)
            except _winreg.error:
                break
            else:
                pnum += 1
                info.append({"Processor": proc})
                phnd = _winreg.OpenKey(chnd, proc)
                pidx = 0
                while True:
                    try:
                        name, value, vtpe = _winreg.EnumValue(phnd, pidx)
                    except _winreg.error:
                        break
                    else:
                        pidx = pidx + 1

                        if isinstance(value, bytes):
                            value = value.rstrip(b'\0')
                            value = auto_decode(value)
                        info[-1][name] = str(value).strip()
                        if name == "Identifier":
                            srch = prgx.search(value)
                            if srch:
                                info[-1]["Family"] = int(srch.group("FML"))
                                info[-1]["Model"] = int(srch.group("MDL"))
                                info[-1]["Stepping"] = int(srch.group("STP"))
    except:
        logger.info('Fail to get cpu info.')
    return info


if __name__ == '__main__':
    # print(get_gpu_info())
    print(get_win32_cpu_info())
