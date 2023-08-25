<template>
<SimplePage>
    <v-card>
      <div style="height: 60px; padding-left: 16px; padding-top: 10px;" class="d-flex my-auto">
        <v-icon color="error" size="44" style="margin-right: 10px">{{ mdiSpeedometer }}</v-icon>
        <span class="text-h4 text-primary" style="margin-top: 3px">Cloudflare 节点选优</span>
      </div>
      <v-divider></v-divider>
      <MarkdownContentBox style="font-size: 18px; line-height: 30px;" :content="mdRaw"/>

      <v-container>
        <v-row>
          <v-col>
            <v-btn block variant="outlined" color="success" @click="optimizeCloudflareHosts()">测速并应用至 hosts</v-btn>
          </v-col>
          <v-col>
            <v-btn block variant="outlined" color="error" @click="removeCloudflareHosts()">移除添加的配置</v-btn>
          </v-col>
        </v-row>
      </v-container>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import {mdiSpeedometer} from '@mdi/js';
import SimplePage from "@/components/SimplePage.vue";
import MarkdownContentBox from "@/components/MarkdownContentBox.vue";
import {useConsoleDialogStore} from "@/store/ConsoleDialogStore";


const mdRaw = `
**Tips 1**: 这个功能仍在实验中，使用前请确保你已经了解足够的网络知识 (至少应该知道如何恢复 hosts 文件，以备不测)。

**Tips 2**: 如果你的默认的连接速度已经足够快，或者你正在使用梯子，那么这个功能对你来说应该没什么用。如果你仍然希望使用这个功能的话，请先关掉梯子，以免干扰测速结果。

**Tips 3**: 这个功能仅对设置中自建服务器下载源生效，因此使用时请切换至自建服务器下载源。

众所周知 Cloudflare 拥有众多边缘节点，而节点的连接速度不一，所以这里提供一个方法来自动选择较好的节点。

之前有在 \`常见问题\` 中提到过 XIU2 大佬分享的 [加速 Cloudflare CDN 访问](https://github.com/XIU2/CloudflareSpeedTest/discussions/71)
的办法, 但这个方法比较复杂, 所以这里提供一个简单些的方式来实现这个功能。

具体就是使用 \`XIU2/CloudflareSpeedTest\` 工具找出连接速度最快的 Cloudflare 节点，并将之应用到 hosts 文件中，以提升下载速度 (~~大概~~。

由于需要修改 hosts 文件，所以需要使用 \`管理员权限\` 才能使这个功能正常运作。

ps. 有时候 Cloudflare 的 ip 可能会被 GFW 阻断(见 [这个 issue](https://github.com/XIU2/CloudflareSpeedTest/issues/217))，
这时候请移除添加的配置再试试，如果还是不行就需要使用梯子了。

pps. 如果无法自动下载 CloudflareST，可以从 [这里](https://pan.baidu.com/s/1Z_wQ_eqx5rd48xgi7DtEsg?pwd=s0x2) 下载然后放到 download 目录。
`
const cds = useConsoleDialogStore()

function optimizeCloudflareHosts() {
  cds.cleanAndShowConsoleDialog()
  window.eel.optimize_cloudflare_hosts()()
}

function removeCloudflareHosts() {
  cds.cleanAndShowConsoleDialog()
  window.eel.remove_cloudflare_hosts()()
}

</script>

<style scoped>

</style>
