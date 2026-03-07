---
id: fastxt-2021-soc
title: Fastxt 2021 SoC
---

## 2021-06-04
<iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/CWEgULrtCxE" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

- Ares reviewed the code for basic UI structure.
- Hill made progress on GitLab CI/CD, with dependency issue.


## 2021-06-02 Fastxt 计划
Ares

### 总体规划

在 2021 年 7 月 18 日之前，应至少完成总体目标中的两项，暂定为以下内容：

（* 标注的为必做，+ 标注的为必须要有大方向上的进展）

* 写 Fastxt 2021 Soc 计划 Blog 和 总结 Blog （*）
* 编写和完善 [fastxt.app](https://fastxt.app) 网站文档 （*）
* 使用 druid 框架实现并取代目前用 Swift UI 实现的 Fastxt 桌面程序 （+）

其中，最后一个目标应在 2021 年 8 月 31 日之前实现。在此之后，10 月 15 日前应完成教程的撰写。

#### 第一阶段规划（5 月 17 日 ~ 7 月 18 日）

* 调研 druid 框架，阅读和模仿开源项目 [psst](https://github.com/jpochyla/psst)
* 完成 Fastxt 的 UI 部分，包含完整的界面，菜单，样式，功能（不包括与 fastxt-core 联动）
* 根据需求调整或补充计划并写一篇总结 Blog
* 编写和完善 [fastxt.app](https://fastxt.app) 网站文档

#### 第二阶段规划（7 月 18 日 ~ 8 月 31 日）

* 实现可以使用的 Windows, Mac 和 Linux 的 发布版本
* 简化代码架构，或优化设计，便于后续教程的撰写
* 确定教程的核心，设计目录与架构
* 完成教程一章的 demo 小样

#### 第三阶段规划（8 月 31 日 ~ 10 月 15 日）

* 完成教程撰写
* 待补充

## 2021-05-28
<iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/rnnRhEsHCPw" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

- Ares made progress on overall UI framework
    - referenced a Spotify client with native GUI written using druid
    - for Fastxt's simple use case, seems no need to have many modules
    - WIP on navigation

TODO
- Ares prepare blog post for plan
- hill to look into CI/CD setup

## 2021-05-22
<iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/bFHRo_wyGps" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

- Ares upgraded `fastxt_core` dependencies
- yi chat about lens and functional programming

## 2021-05-15
<iframe width="560" height="315" src="https://www.youtube-nocookie.com/embed/K4GiRH_B82o" title="YouTube video player" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture" allowfullscreen></iframe>

- introduction
- Ares mentioned gRPC (tonic, prost)

## 2021-04-23
和 [Local Native 2021 Summer of Code](https://localnative.app/blog/2021/04/01/localnative-2021-soc-team)类似, 但是使用 druid 这个 Rust GUI 框架来推进 Fastxt 和编写相关课程。

### 时间线

日期| 事件 | Note
------|-----|-----
May 17 - June 7 | 熟悉代码和框架，写计划 Blog |
June 7 - July 18 | code() and debug() and document() | July 18  Evaluation
July 18 - August 16 | code() and debug() and document() |
August 16-31 | 写总结 Blog 和 educative 教程文档 | August 31 Evaluation
TBD | educative 审核上线 |

### 成员和主要目标

- Ares
  - 使用 druid 框架实现并取代目前用 Swift UI 实现的 Fastxt 桌面程序
  - 实现可以使用的 Windows, Mac 和 Linux 的 发布版本
  - 在 fastxt.app 网站 Blog 上写在 educative.io 上Fastxt课程关于Rust GUI 的中文版
  - 根据 educative.io 平台的反馈持续修改 Faxtxt 课程内容使其通过课程的审核和上线发布
  - 编写和完善 https://fastxt.app 网站文档
  - 写Fastxt 2021 SoC计划 Blog 和总结 Blog

- hill
  - 协助实现代码和技术文档等工作
