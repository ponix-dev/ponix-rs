## [1.13.0](https://github.com/ponix-dev/ponix-rs/compare/v1.12.1...v1.13.0) (2025-12-31)

### Features

* add gateway_id to end devices ([#116](https://github.com/ponix-dev/ponix-rs/issues/116)) ([d5c57ee](https://github.com/ponix-dev/ponix-rs/commit/d5c57ee8fb622001ab9ae0f4abb8edb671116fd4))

## [1.12.1](https://github.com/ponix-dev/ponix-rs/compare/v1.12.0...v1.12.1) (2025-12-30)

### Code Refactoring

* scope device queries by workspace ([#115](https://github.com/ponix-dev/ponix-rs/issues/115)) ([be63e8e](https://github.com/ponix-dev/ponix-rs/commit/be63e8e6beacd57f3d5a5c9f7839dd2db865665d))

## [1.12.0](https://github.com/ponix-dev/ponix-rs/compare/v1.11.0...v1.12.0) (2025-12-30)

### Features

* add Workspace entity for grouping end devices ([#113](https://github.com/ponix-dev/ponix-rs/issues/113)) ([2b01b19](https://github.com/ponix-dev/ponix-rs/commit/2b01b1944145766d5a70ea1e8073893c7de0634e))

## [1.11.0](https://github.com/ponix-dev/ponix-rs/compare/v1.10.0...v1.11.0) (2025-12-23)

### Features

* adds json schema validation for processed envelopes ([#101](https://github.com/ponix-dev/ponix-rs/issues/101)) ([d7bbe46](https://github.com/ponix-dev/ponix-rs/commit/d7bbe46e9e3e5f2949257a0d438f950ac5c2c829))

## [1.10.0](https://github.com/ponix-dev/ponix-rs/compare/v1.9.2...v1.10.0) (2025-12-22)

### Features

* adds end device definition ([#100](https://github.com/ponix-dev/ponix-rs/issues/100)) ([f8e747b](https://github.com/ponix-dev/ponix-rs/commit/f8e747b51fae62b1d06c1cb197df2af4bf3b6e26))

## [1.9.2](https://github.com/ponix-dev/ponix-rs/compare/v1.9.1...v1.9.2) (2025-12-21)

### Code Refactoring

* adds garde for validation ([#96](https://github.com/ponix-dev/ponix-rs/issues/96)) ([d09a529](https://github.com/ponix-dev/ponix-rs/commit/d09a52965b2ee90903c18f8b7352fc676a6c6e58))

## [1.9.1](https://github.com/ponix-dev/ponix-rs/compare/v1.9.0...v1.9.1) (2025-12-21)

### Code Refactoring

* decouples service and repo inputs ([#95](https://github.com/ponix-dev/ponix-rs/issues/95)) ([2c24b6b](https://github.com/ponix-dev/ponix-rs/commit/2c24b6b564c2e9b63ff58b6a64fdac2a67799e3a))

## [1.9.0](https://github.com/ponix-dev/ponix-rs/compare/v1.8.0...v1.9.0) (2025-12-21)

### Features

* adds org id for rpc requests ([#90](https://github.com/ponix-dev/ponix-rs/issues/90)) ([4c29615](https://github.com/ponix-dev/ponix-rs/commit/4c29615b535ad8d94b534aaf15a0225c197db3ac))
* Implement Org-Scoped RBAC with Casbin ([#91](https://github.com/ponix-dev/ponix-rs/issues/91)) ([62e9c8e](https://github.com/ponix-dev/ponix-rs/commit/62e9c8ea05e8304e725a6b8837d20b7d71185a32))

## [1.8.0](https://github.com/ponix-dev/ponix-rs/compare/v1.7.1...v1.8.0) (2025-12-08)

### Features

* adds user org listing ([#85](https://github.com/ponix-dev/ponix-rs/issues/85)) ([57c7710](https://github.com/ponix-dev/ponix-rs/commit/57c7710a4eff66c74de31e00aaede82248518ea3))

## [1.7.1](https://github.com/ponix-dev/ponix-rs/compare/v1.7.0...v1.7.1) (2025-12-07)

### Bug Fixes

* CORS configuration for credentials and cookie handling ([#83](https://github.com/ponix-dev/ponix-rs/issues/83)) ([5a0af5b](https://github.com/ponix-dev/ponix-rs/commit/5a0af5bc025a60d4393b68335cf7914d35753d4a))

## [1.7.0](https://github.com/ponix-dev/ponix-rs/compare/v1.6.0...v1.7.0) (2025-12-07)

### Features

* Add refresh token support for authentication ([#81](https://github.com/ponix-dev/ponix-rs/issues/81)) ([70d56af](https://github.com/ponix-dev/ponix-rs/commit/70d56afd06566521272b7b62c4b1bb41c9385adf))

## [1.6.0](https://github.com/ponix-dev/ponix-rs/compare/v1.5.1...v1.6.0) (2025-12-05)

### Features

* Connect users to organizations on creation ([#79](https://github.com/ponix-dev/ponix-rs/issues/79)) ([77d6338](https://github.com/ponix-dev/ponix-rs/commit/77d63381965a87cc4697a6212895da42080a3386))

## [1.5.1](https://github.com/ponix-dev/ponix-rs/compare/v1.5.0...v1.5.1) (2025-12-05)

### Code Refactoring

* moves auth to common crate ([#78](https://github.com/ponix-dev/ponix-rs/issues/78)) ([7843b13](https://github.com/ponix-dev/ponix-rs/commit/7843b13ab196180744fa480f7e347b58a4da2252))

## [1.5.0](https://github.com/ponix-dev/ponix-rs/compare/v1.4.0...v1.5.0) (2025-12-04)

### Features

* adds user login ([9b9b901](https://github.com/ponix-dev/ponix-rs/commit/9b9b90110e2107cae9d701afc596628ebca7f6fc))

## [1.4.0](https://github.com/ponix-dev/ponix-rs/compare/v1.3.0...v1.4.0) (2025-12-04)

### Features

* adds get and register user rpcs ([6f9d8f1](https://github.com/ponix-dev/ponix-rs/commit/6f9d8f1f8ff20175d2ff140d65dee8c9845d3297))

## [1.3.0](https://github.com/ponix-dev/ponix-rs/compare/v1.2.0...v1.3.0) (2025-11-30)

### Features

* adds list organization endpoint ([26bab27](https://github.com/ponix-dev/ponix-rs/commit/26bab2756de7fe5893881046f7ffd1db25e1d57e))

## [1.2.0](https://github.com/ponix-dev/ponix-rs/compare/v1.1.0...v1.2.0) (2025-11-30)

### Features

* adds grpc-web support ([38cd884](https://github.com/ponix-dev/ponix-rs/commit/38cd884ed658c4a73f5655446f685dca6180ad9b))

## [1.1.0](https://github.com/ponix-dev/ponix-rs/compare/v1.0.1...v1.1.0) (2025-11-30)

### Features

* adds emqx to docker compose ([c8215d1](https://github.com/ponix-dev/ponix-rs/commit/c8215d1e11cecd85c2c3c12d62d5ede1ec04bfc8))
* adds otel tracing ([36eb1f7](https://github.com/ponix-dev/ponix-rs/commit/36eb1f77d328900ac827807e35bfe13bb491eba9))
* adds shared subscription support ([18a24ba](https://github.com/ponix-dev/ponix-rs/commit/18a24ba726e0a758cd7f2ff431cdd945281e8af8))
* initial mqtt support ([34ce731](https://github.com/ponix-dev/ponix-rs/commit/34ce731a8ec9baf14c944c828d83564ef4ab72b2))

### Bug Fixes

* integration tests ([d9bb62c](https://github.com/ponix-dev/ponix-rs/commit/d9bb62cf68d9f4ddc51abfa8cfdc38257badcc39))

### Code Refactoring

* config validaiton for gateway creation ([bcabbae](https://github.com/ponix-dev/ponix-rs/commit/bcabbae6a1c7aeaef9eac3f81de87880531cfab7))
* instument payload logic ([879ee19](https://github.com/ponix-dev/ponix-rs/commit/879ee19f359a07376d46ff84f0ed1d52a0f0f0db))
* removes cyclical dependency in gateway orchestrator ([9d3cc25](https://github.com/ponix-dev/ponix-rs/commit/9d3cc251486a8b3cdd918b67caf893f5dcf9d724))

## [1.0.1](https://github.com/ponix-dev/ponix-rs/compare/v1.0.0...v1.0.1) (2025-11-28)

### Code Refactoring

* adds nats tower layer ([5c797fb](https://github.com/ponix-dev/ponix-rs/commit/5c797fb3dbb38ab57b8eb43ee60c2ca4e04913fb))
* moves cdc consumer to use nats tower ([fadcf47](https://github.com/ponix-dev/ponix-rs/commit/fadcf47875a868bb6986484bab01dd7a0316387b))
* nats tower consumer implementation ([650734c](https://github.com/ponix-dev/ponix-rs/commit/650734cf1cd5ff71a8553eec9834e2c049f2c2b9))
* removes old nats consuming implementation ([e118b01](https://github.com/ponix-dev/ponix-rs/commit/e118b0103b972ea9e5b1b052da6242fb25af8bf9))

## 1.0.0 (2025-11-28)

### Features

* Add mockall support and unit tests for ponix-nats package ([#11](https://github.com/ponix-dev/ponix-rs/issues/11)) ([caee13b](https://github.com/ponix-dev/ponix-rs/commit/caee13be7741db2e884902567b78a987202cda27))
* adds cayenne lpp json conversion ([0adaed1](https://github.com/ponix-dev/ponix-rs/commit/0adaed1aba606bb5e596f6bbf82a581ce2a281dd))
* adds cel payload env for converting payloads to json ([7725125](https://github.com/ponix-dev/ponix-rs/commit/7725125eae70faba777ee9bb69877e4b4ad52671))
* adds claude.md ([6dbe111](https://github.com/ponix-dev/ponix-rs/commit/6dbe111872bce936072d4b59314e9c6e0796ad4c))
* adds domain crate with end device service ([1614b4c](https://github.com/ponix-dev/ponix-rs/commit/1614b4c0b36354e68ba3e7f5dc8a725d2c454986))
* adds grpc endpoints ([641fa01](https://github.com/ponix-dev/ponix-rs/commit/641fa0139a0adb69b101178be839eb2ab5b19c26))
* adds grpc integration ([90c064f](https://github.com/ponix-dev/ponix-rs/commit/90c064ff7ab75361541a1415bab9c4d8d7063709))
* adds loki integration ([995f336](https://github.com/ponix-dev/ponix-rs/commit/995f3368a0e65943e213626cd57bdf076eaa7d0b))
* adds migrations ([df3d924](https://github.com/ponix-dev/ponix-rs/commit/df3d9242cf7a83c930a8af1d31bbc0d552d160aa))
* adds migrations ([09d6355](https://github.com/ponix-dev/ponix-rs/commit/09d635554cba5e0ccf2e3e6915e5575951733f29))
* adds nats implementations for raw envelopes ([7b939ed](https://github.com/ponix-dev/ponix-rs/commit/7b939ed7b4f987ffb6b52da51e3b00378301e7e2))
* adds payload conversion to end device ([c51a5ad](https://github.com/ponix-dev/ponix-rs/commit/c51a5ad1546198558a8fdafd847b6c6749faa521))
* adds plan ([df73e08](https://github.com/ponix-dev/ponix-rs/commit/df73e0891e5483a086b654b93477afe7b1fa0f87))
* adds postgres imp ([74a4420](https://github.com/ponix-dev/ponix-rs/commit/74a4420ce00f1fc6d1f9907106603641fc29dc99))
* adds processed envelope producer and consumer ([#9](https://github.com/ponix-dev/ponix-rs/issues/9)) ([109220e](https://github.com/ponix-dev/ponix-rs/commit/109220ed264f4be71302ff6088f607accf2545eb))
* adds proper config to dc ([c816fd2](https://github.com/ponix-dev/ponix-rs/commit/c816fd290d5b34f713789b591114dc4249767673))
* adds protobuf support ([0bf7123](https://github.com/ponix-dev/ponix-rs/commit/0bf712318d7ff5fe9a3b9ff090a239b8baec2463))
* adds query for getting all gateways ([0e6d3aa](https://github.com/ponix-dev/ponix-rs/commit/0e6d3aa9279d9c4afa80f1ceab742815f4af212d))
* adds raw to processed envelope flow ([2fe299e](https://github.com/ponix-dev/ponix-rs/commit/2fe299e9d4b1d5ec48b2ad9c337a760718d8d7c9))
* adds rpc logging ([f8bfaad](https://github.com/ponix-dev/ponix-rs/commit/f8bfaad2ccbbe5738197e88065c714a93e531432))
* adds service scaffold ([ad3b16a](https://github.com/ponix-dev/ponix-rs/commit/ad3b16aaefc33d678123176729e77b7cd8c8e7c2))
* adds span and trace id to logs ([beb6277](https://github.com/ponix-dev/ponix-rs/commit/beb6277aeba755415ea8e62d4c6486bb2d151447))
* adds tower trace middleware ([5e6bbf2](https://github.com/ponix-dev/ponix-rs/commit/5e6bbf2044fca596cf5a922cf395c0abe2fffd10))
* **clickhouse:** adds async writer for processed env nats consumer ([125caaa](https://github.com/ponix-dev/ponix-rs/commit/125caaa25e5dffa6ef979f3feb36aa9064a47ba3))
* confirms org exists before end device creation ([a996440](https://github.com/ponix-dev/ponix-rs/commit/a996440b9247612207123eff0c69788856d413f5))
* dynamic process orchestrator ([3178886](https://github.com/ponix-dev/ponix-rs/commit/3178886d203b8fb2243686c0ff7abdd10d63a758))
* extended cayenne lpp support ([da1278f](https://github.com/ponix-dev/ponix-rs/commit/da1278f7fa3291a143a31df92a0b970fe5c53958))
* **grpc:** adds end device server impl ([3cc3897](https://github.com/ponix-dev/ponix-rs/commit/3cc38971f0678bc45cc407ce706610bbcce56da4))
* implements repository ([586b32c](https://github.com/ponix-dev/ponix-rs/commit/586b32c2f54947b12a2f79e36e639603d01d7de0))
* initial gateway orchestrator ([70c4d55](https://github.com/ponix-dev/ponix-rs/commit/70c4d5511de88bbb23f7245a2f2c0bc224d5c85c))
* initial tracing ([d11ae5f](https://github.com/ponix-dev/ponix-rs/commit/d11ae5f6db8391bb72dfbf36a18dcd7c98e3cfe4))
* integrates postgres in to ponix-all-in-one ([12e6d3d](https://github.com/ponix-dev/ponix-rs/commit/12e6d3d68435178cee0c6664800404bce5450cee))
* **ponix-all-in-one:** adds grpc server ([86bccd4](https://github.com/ponix-dev/ponix-rs/commit/86bccd466b2b4c04e207c952b732ec8c9a80bd1b))
* sets up full ingestion pipeline ([5ee81ea](https://github.com/ponix-dev/ponix-rs/commit/5ee81eac372dade28b256fd1484d5e2ed483f556))
* updates domain ([6041ab3](https://github.com/ponix-dev/ponix-rs/commit/6041ab391efd2b156c2a47aea5af33f1096dd961))

### Bug Fixes

* adds docker values ([c110ce4](https://github.com/ponix-dev/ponix-rs/commit/c110ce44edf031a3aa266aef91a4fb1d168ea445))
* grpc reflection ([fed0519](https://github.com/ponix-dev/ponix-rs/commit/fed05194ef7b3ba76ef9b1a0900f7738115657b4))
* unit tests ([bbd4316](https://github.com/ponix-dev/ponix-rs/commit/bbd4316cf4a4302112f5dc5ca3b2bc713c930a6c))

### Code Refactoring

* adds goose crate ([dfad5c5](https://github.com/ponix-dev/ponix-rs/commit/dfad5c5ed0fbcdffe0d40190f28d8007128d7118))
* breaks apart ponix-all-in-one setup ([dc8486f](https://github.com/ponix-dev/ponix-rs/commit/dc8486f1f4899fed5ec0b83a6e74fbb43fe6c323))
* broke apart processes to support future microservices ([2d409ab](https://github.com/ponix-dev/ponix-rs/commit/2d409abdcee1795678e27f866951de5fcde74892))
* cleanup ([5fe7e88](https://github.com/ponix-dev/ponix-rs/commit/5fe7e886a9087a8a52a88846d4efd07263f6dc87))
* domain processed envelope flow ([fb639b6](https://github.com/ponix-dev/ponix-rs/commit/fb639b64ae09647d6caf5c580df1bce0016e6379))
* fmt ([c6ecd94](https://github.com/ponix-dev/ponix-rs/commit/c6ecd94c48ac16fa5a779ad7b6c250bc76738367))
* json logging ([5a18988](https://github.com/ponix-dev/ponix-rs/commit/5a189884fbc6d7bfd56c8bc8f85e0364f7a5541d))
* make cdc entities configurable ([ef0901f](https://github.com/ponix-dev/ponix-rs/commit/ef0901fffe562aa09655ea809b51a47b6c15cae3))
* **nats:** decouples protobuf decoupling and bussiness logic handling ([#13](https://github.com/ponix-dev/ponix-rs/issues/13)) ([e83592d](https://github.com/ponix-dev/ponix-rs/commit/e83592db08f77d8d23fbb5855f8df367c82aa4f4))
* passes in cdc config ([f4706d0](https://github.com/ponix-dev/ponix-rs/commit/f4706d08416878d2d4d5be0250c9b8ad72fc8200))
* **ponix-postgres:** implements domain repo trait ([9424749](https://github.com/ponix-dev/ponix-rs/commit/9424749bf6dc10309dcb7788d46dea5d92d19193))
* redoes common exports ([37ee09d](https://github.com/ponix-dev/ponix-rs/commit/37ee09dbbe226bb3069c65badd56df471600c525))
* removes mod.rs files ([6687242](https://github.com/ponix-dev/ponix-rs/commit/6687242f1ca45178aa6e38e9facb9ecf1a43d8f0))
* updates domain layer ([9d3287c](https://github.com/ponix-dev/ponix-rs/commit/9d3287c1732117d6eb183181f347570e937d0bac))
* updates logs ([b556c91](https://github.com/ponix-dev/ponix-rs/commit/b556c918547806df137f999b46982d7343ffde62))
* uses goose crate for clickhouse ([0a4b31a](https://github.com/ponix-dev/ponix-rs/commit/0a4b31af2c174517a17ef82be96a4e131aac3970))
