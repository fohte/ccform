# Changelog

## [0.1.1](https://github.com/fohte/ccform/compare/v0.1.0...v0.1.1) (2026-09-04)


### Features

* **cli:** add DiffReport renderer ([#34](https://github.com/fohte/ccform/issues/34)) ([7f8e315](https://github.com/fohte/ccform/commit/7f8e315891f9ec9297aa52fef3c5afbf59ceea4d))
* **cli:** add subcommand skeleton ([#31](https://github.com/fohte/ccform/issues/31)) ([86265f6](https://github.com/fohte/ccform/commit/86265f6f5bb4a7c47d72341ea2bf244ab19660f4))
* **cli:** implement the `ccform init` subcommand ([#35](https://github.com/fohte/ccform/issues/35)) ([581ee7f](https://github.com/fohte/ccform/commit/581ee7f4ee25b9d4b1b214bfbdb095ee196b7349))
* **cli:** implement the `ccform show` subcommand ([#37](https://github.com/fohte/ccform/issues/37)) ([ba1b305](https://github.com/fohte/ccform/commit/ba1b3053194de738f543651de64385c4c01cfb98))
* **config/lua:** add Lua VM runtime ([#5](https://github.com/fohte/ccform/issues/5)) ([6f3b3c8](https://github.com/fohte/ccform/commit/6f3b3c8dff7f3b6471fa9041e1fc2c30c01ebaa0))
* **config:** add ccform.merge to compose multiple tables ([#22](https://github.com/fohte/ccform/issues/22)) ([9e5c1e3](https://github.com/fohte/ccform/commit/9e5c1e3a9d73dec9d0b2e47cc774344741b1c2f7))
* **config:** add ccform.replace value marker ([#21](https://github.com/fohte/ccform/issues/21)) ([582ecba](https://github.com/fohte/ccform/commit/582ecba45c5c5153be4c256634950df3f7e11fb6))
* **config:** add JSON-to-Lua literal serializer ([#23](https://github.com/fohte/ccform/issues/23)) ([d657d21](https://github.com/fohte/ccform/commit/d657d21abee87fc5171aeb3c34fa8c9672de8f7a))
* **config:** allow ccform.lua to read environment variables ([#19](https://github.com/fohte/ccform/issues/19)) ([a3bcb2e](https://github.com/fohte/ccform/commit/a3bcb2e8abe83c393f61f513db76c4f4cd6228cb))
* **config:** convert values between Lua and JSON ([#27](https://github.com/fohte/ccform/issues/27)) ([d57d5d9](https://github.com/fohte/ccform/commit/d57d5d99c4e921654df780db4945794e08f3cd08))
* **config:** merge import.lua ahead of ccform.lua automatically ([#28](https://github.com/fohte/ccform/issues/28)) ([6183831](https://github.com/fohte/ccform/commit/6183831053e4108ac3d4bbd78d5ccc5118e85d97))
* **config:** validate and partition ccform.lua return values ([#26](https://github.com/fohte/ccform/issues/26)) ([e9173a2](https://github.com/fohte/ccform/commit/e9173a2c8617a217354ebe414924b55ff6a538c0))
* **io:** add atomic write utilities ([#8](https://github.com/fohte/ccform/issues/8)) ([2f8a35f](https://github.com/fohte/ccform/commit/2f8a35f30492e565265bee533f9a8107151a6760))
* **paths:** add XDG-aware path resolution utilities ([#4](https://github.com/fohte/ccform/issues/4)) ([30462b7](https://github.com/fohte/ccform/commit/30462b7e933c2c0d60bbf91ed2c92e153f49f639))
* **state:** add a state.json store ([#30](https://github.com/fohte/ccform/issues/30)) ([21259b6](https://github.com/fohte/ccform/commit/21259b6252becd016693eef20fe34841b8244309))
* **state:** compute a 3-way diff for plan/drift/import ([#29](https://github.com/fohte/ccform/issues/29)) ([77a22ab](https://github.com/fohte/ccform/commit/77a22abfa19734769a51003305cb75024e481693))
* **target:** add mcpServers partial-update target ([#24](https://github.com/fohte/ccform/issues/24)) ([e6950ee](https://github.com/fohte/ccform/commit/e6950eebfdea24b02fdc20110cb69628fb531331))
* **target:** add settings.json read/write target ([#25](https://github.com/fohte/ccform/issues/25)) ([1b9f9d8](https://github.com/fohte/ccform/commit/1b9f9d84accd4fe085617561a04f2e353ac6dc01))


### Dependencies

* update rust crate anyhow to v1.0.104 ([#44](https://github.com/fohte/ccform/issues/44)) ([d3c2f44](https://github.com/fohte/ccform/commit/d3c2f447d371e453096cefaf94349d79a4c8ee55))
* update rust crate clap to v4.6.2 ([#41](https://github.com/fohte/ccform/issues/41)) ([2930491](https://github.com/fohte/ccform/commit/2930491de5203d58e368f1f1d28809610ca785a9))
* update rust crate clap to v4.6.3 ([#50](https://github.com/fohte/ccform/issues/50)) ([bf141bd](https://github.com/fohte/ccform/commit/bf141bdb226ea97f46f5155fd4af7eded2941c41))
* update rust crate clap to v4.6.4 ([#53](https://github.com/fohte/ccform/issues/53)) ([55e656d](https://github.com/fohte/ccform/commit/55e656d7c63ef6f92d70a35a69ea2badddb8f999))
* update rust crate clap to v4.6.5 ([#73](https://github.com/fohte/ccform/issues/73)) ([2b5ed64](https://github.com/fohte/ccform/commit/2b5ed6441aa737097cbc08d677351c8ecbf1880c))
* update rust crate clap to v4.6.6 ([#79](https://github.com/fohte/ccform/issues/79)) ([c320553](https://github.com/fohte/ccform/commit/c3205533ecb0d31d28ec1669f634985dcb65f570))
* update rust crate serde to v1.0.229 ([#45](https://github.com/fohte/ccform/issues/45)) ([03a26a7](https://github.com/fohte/ccform/commit/03a26a7b5113b4ec79e8ae9d3292d01f63b793aa))
* update rust crate serde_json to v1.0.151 ([#48](https://github.com/fohte/ccform/issues/48)) ([e46ce87](https://github.com/fohte/ccform/commit/e46ce87d4fbf3fb37ea29f10ef07f7222ced9b6a))
* update rust crate thiserror to v2.0.19 ([#46](https://github.com/fohte/ccform/issues/46)) ([8ab52ba](https://github.com/fohte/ccform/commit/8ab52badf357d5675fd7984ea5dd82047fc1d3e3))
* update rust crate thiserror to v2.0.20 ([#82](https://github.com/fohte/ccform/issues/82)) ([e864307](https://github.com/fohte/ccform/commit/e864307155611a8817f482b026268ecec4f53a92))
