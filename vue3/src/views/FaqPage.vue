<template>
  <SimplePage>
    <v-card id="faq-card" style="padding-bottom: 10px">
      <v-card-title>
        <span class="text-h4 text-primary">常见问题</span>
      </v-card-title>
      <v-divider></v-divider>
      <v-list>
        <FaqGroup>
          <template v-slot:title>第一次使用应该安装哪些组件</template>
          <template v-slot:content>
            <p>1. 安装最新的显卡驱动，这能减少模拟器运行时发生的很多问题。</p>
            <p>2. 安装 Yuzu/Ryujinx 模拟器（如果检测到缺少 msvc 运行库的话会自动运行 msvc 的安装程序，msvc
              装完后记得重启一下）</p>
            <p>3. 安装固件并配置相应的密钥文件</p>
            <p>这些东西装完之后模拟器就安装好了。</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>Yuzu/Ryujinx 最新版本信息加载失败</template>
          <template v-slot:content>
            <p>yuzu/ryujinx 的 release 数据需要从 GitHub api 中获取，而接口调用失败时就会出现这个问题。</p>
            <p><b>这个问题可能由以下两种原因产生:</b></p>
            <p>1.某些运营商的屏蔽了 api.github.com 这个地址，因此无法连接至 GitHub.</p>
            <p>2.如果你使用的是共享的 ip (比如用了某些公用的梯子或某些运营商的公共出口)，可能是当前 ip 使用的频率达到
              GitHub 的使用上限(大概每小时 60 次).</p>
            <p><b>解决方法:</b></p>
            <p>1. 在设置中将 GitHub api 改为 "使用 CDN"；如果还是不行，那说明 CDN 也被屏蔽了，挂个梯子直连吧。</p>
            <p>2. 如果是用梯子的话可以尝试换个节点, 或者等一段时间, GitHub 会自动解除封禁（最多只封 1h）</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>下载时出错，download 文件夹下没有任何东西</template>
          <template v-slot:content>
            <p>特殊地区/时期 Cloudflare
              服务器会因为某些原因被屏蔽，可以在设置中切换下载源，如果还是不行就需要使用代理软件了。</p>
            <p>或者前往下一小节中提到的 整合贴 中下载整合包。</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>下载速度很慢</template>
          <template v-slot:content>
            <p><b>如果设置中的下载源用的是 "直连":</b></p>
            <p>请直接和你的宽带运营商或者梯子提供者反馈下载速度不达标的问题。</p>
            <p><b>如果设置中的下载源用的是 CDN:</b></p>
            <p>
              由于 Cloudflare CDN 服务器不在国内，因此下载速度取决于你的宽带运营商所能提供的国际带宽，
              某些运营商为了节约运营成本，提供的国际线路不太好，带宽容量也比较少，所以在高峰期会出现丢包和卡顿。
            </p>
            <p>一劳永逸的解决办法当然是用好点的运营商以及一个好点的梯子
              <del>(加钱，世界触手可及.jpg)</del>
            </p>
            <p>如果你的动手能力比较强，可以看看这个由 XIU2 大佬分享的 <a
                @click="openUrlWithDefaultBrowser('https://github.com/XIU2/CloudflareSpeedTest/discussions/71')">
              加速 Cloudflare CDN 访问</a> 的办法。</p>
            <p>或者直接在贴吧的
              <a @click="openUrlWithDefaultBrowser('https://tieba.baidu.com/p/7665223775')">整合贴 1</a> 或
              <a @click="openUrlWithDefaultBrowser('https://tieba.baidu.com/p/7799545671')">整合贴 2</a>
              中下载整合包。
            </p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>如何为 aria2 配置代理</template>
          <template v-slot:content>
            <p>目前程序可以检测系统代理，并在进行下载时自动为 aria2 配置代理。</p>
            <p>当前使用的系统代理可以在右键 开始菜单图标 - 设置 - 网络和 Internet - 代理 - 代理服务器 中查看。</p>
            <p>v2rayN 的 "自动配置系统代理" 以及 Clash for windows 的 "System Proxy" 都可以正确配置系统代理。</p>
            <p>其它代理工具请自行摸索。</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>点击安装后出现 “当前的 xxx 就是 [yyy], 跳过安装.”</template>
          <template v-slot:content>
            <p>这是由于记录中的版本和你选择的版本一致，为了避免重复下载/安装，这里会跳过安装过程。</p>
            <p>如果你确认你选择的版本没有安装过，那可能是因为你用的是别人的配置文件。</p>
            <p>可以删除目录下的 config.json 文件，然后重启程序(这会重置你的设置及记录).</p>
            <p>这个时候点安装就应该正常了。</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>关于模拟器版本检测</template>
          <template v-slot:content>
            <p>
              模拟器版本检测的原理是启动模拟器，然后根据窗口标题确定正在使用的模拟器是什么版本，再将检测到的版本和分支信息保存到程序的记录中。</p>
            <p>ps.这个功能仅用于更新记录的版本号，不影响模拟器及程序的使用。</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>关于固件版本检测</template>
          <template v-slot:content>
            <p>
              固件版本检测的原理是使用配置的密钥去解密固件文件，从而获取版本号。因此，这个功能需要你正确的配置了固件和相应的密钥后才能使用</p>
            <p>ps.这个功能仅用于更新记录的版本号，不影响程序的使用。</p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>游戏与模拟器的兼容问题</template>
          <template v-slot:content>
            <p>
              不同游戏对不同版本的模拟器兼容程度不一样，新版本模拟器对老游戏的支持不一定好，可以在贴吧里面找找别人是用什么版本
              的模拟器和固件通关的，依此来决定你应该使用什么版本。
            </p>
            <p>
              对于新游戏，模拟器会渐渐完善相关的支持，可以等一段时间后更新试试。
            </p>
          </template>
        </FaqGroup>

        <FaqGroup>
          <template v-slot:title>其它问题反馈</template>
          <template v-slot:content>
            <p>如果你遇到的问题不属于上面的任何一个，可以在
              <a
                 @click="openUrlWithDefaultBrowser('https://github.com/triwinds/ns-emu-tools/issues')">GitHub Issues</a>
              中提交问题反馈，记得带上程序目录下的两个 log 文件，这将有助于排查你遇到的问题。
            </p>
          </template>
        </FaqGroup>
      </v-list>
    </v-card>
  </SimplePage>
</template>

<script setup lang="ts">
import SimplePage from "@/components/SimplePage.vue";
import FaqGroup from "@/components/FaqGroup.vue";
import {openUrlWithDefaultBrowser} from "@/utils/common";
import {onMounted} from "vue";

onMounted(() => {
  let aLinks = document.getElementById('faq-card')?.getElementsByTagName('a')
  if (aLinks) {
    for (let aLink of aLinks) {
      aLink.classList.add('text-primary')
      aLink.style.cursor = 'pointer'
    }
  }
})

</script>

<style scoped>
p {
  font-size: 18px;
  line-height: 30px !important;
  margin-bottom: 16px !important;
}
</style>
