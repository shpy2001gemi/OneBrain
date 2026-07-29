// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Vietnamese (`vi`).
class AppLocalizationsVi extends AppLocalizations {
  AppLocalizationsVi([String locale = 'vi']) : super(locale);

  @override
  String get appTitle => 'OneBrain';

  @override
  String get bootstrapEyebrow => 'NODE DI ĐỘNG TỰ CHỦ';

  @override
  String get entryResolving => 'Đang mở node này…';

  @override
  String get welcomeTitle => 'Nuôi dưỡng ý tưởng trên node của bạn';

  @override
  String get welcomeBody =>
      'Điện thoại này là một node OneBrain độc lập. Đây không phải ứng dụng đi kèm desktop và có thể giữ công việc riêng tư ngay trên thiết bị.';

  @override
  String get languageTitle => 'Chọn ngôn ngữ giao diện';

  @override
  String get languageEnglish => 'English';

  @override
  String get languageVietnamese => 'Tiếng Việt';

  @override
  String get nodeFactTitle => 'Ranh giới của node';

  @override
  String get nodeFactBody =>
      'Flutter trình bày trạng thái có kiểu. Lớp native và Rust sở hữu tác vụ nền tảng, dữ liệu bền vững, chính sách, chữ ký và công cụ.';

  @override
  String get registryFactTitle => 'Dữ liệu tri thức bắt buộc';

  @override
  String get registryFactBody =>
      'Concept Registry không được đóng gói trong ứng dụng. Luồng Init sau này chỉ tải dữ liệu sau khi người dùng xem kế hoạch và xác nhận rõ ràng.';

  @override
  String get requestFactTitle => 'Mạng trước Init';

  @override
  String get requestFactBody =>
      'Màn hình khởi tạo này không gửi yêu cầu tải artifact Registry.';

  @override
  String get nativeHostTitle => 'Native host';

  @override
  String get nativeHostLoading => 'Đang kiểm tra host của thiết bị…';

  @override
  String nativeHostReady(String platform, String apiVersion) {
    return 'Host $platform sẵn sàng · API $apiVersion';
  }

  @override
  String get nativeHostUnavailable =>
      'Native host không khả dụng trong môi trường này. Giao diện khởi tạo vẫn dùng được.';

  @override
  String get rustBridgeTitle => 'Cầu nối Rust';

  @override
  String get rustBridgeLoading => 'Đang kiểm tra ranh giới native–Rust…';

  @override
  String rustBridgeReady(String coreVersion, int abiVersion) {
    return 'Cầu nối Rust $coreVersion · ABI $abiVersion';
  }

  @override
  String get rustBridgeUnavailable =>
      'Cầu nối Rust chưa được liên kết trong bản build này. Không tuyên bố runtime sẵn sàng.';

  @override
  String get rustBridgeVerified => 'Đã xác minh round trip có kiểu';

  @override
  String get rustBridgeNotVerified => 'Chưa xác minh được round trip Rust';

  @override
  String get mobileRuntimeTitle => 'Hồ sơ runtime di động';

  @override
  String get mobileRuntimeLoading => 'Đang mở runtime BootstrapOnly cục bộ…';

  @override
  String get mobileRuntimeUnavailable =>
      'Không thể mở runtime cục bộ. Chưa tuyên bố khả năng sẵn sàng ngoại tuyến.';

  @override
  String mobileRuntimeReady(
    String profileVersion,
    int generation,
    String phase,
    int grantCount,
    String registryState,
  ) {
    return 'Hồ sơ $profileVersion · thế hệ $generation · $phase với $grantCount quyền thực thi đang hoạt động. Trạng thái Registry: $registryState. Danh tính gắn với thiết bị và vault mã hóa đang hoạt động.';
  }

  @override
  String mobileRuntimeRecovered(int generation) {
    return 'Đã phục hồi thế hệ $generation sau khi tiến trình trước kết thúc mà không có callback quiesce. Callback cũ vẫn bị chặn.';
  }

  @override
  String get mobileRuntimeVerified =>
      'Đã xác minh danh tính được bảo vệ, vault mã hóa và runtime cục bộ';

  @override
  String get mobileRuntimeNotVerified => 'Chưa hoàn tất xác minh hồ sơ runtime';

  @override
  String get continueAction => 'Tiếp tục kiểm tra thiết bị';

  @override
  String get galleryAction => 'Xem component dùng chung';

  @override
  String get galleryTitle => 'Thư viện component dùng chung';

  @override
  String get galleryBody =>
      'Các điều khiển này bọc primitive Material 3 và dùng lại semantic token của OneBrain.';

  @override
  String get primaryButton => 'Hành động chính';

  @override
  String get tonalButton => 'Hành động tonal';

  @override
  String get outlineButton => 'Hành động viền';

  @override
  String get statusReady => 'Giao diện khởi tạo sẵn sàng';

  @override
  String get statusWaiting => 'Chưa bắt đầu Registry Init';

  @override
  String get statusPrivate => 'Phạm vi riêng tư cục bộ';

  @override
  String get backAction => 'Quay lại';

  @override
  String get notImplementedTitle => 'Bước nền tảng tiếp theo';

  @override
  String get notImplementedBody =>
      'Kiểm tra dung lượng thiết bị và kế hoạch Init có xác nhận chưa được mô phỏng trong lát cắt này.';

  @override
  String onboardingStep(int current, int total) {
    return 'Bước $current / $total';
  }

  @override
  String get onboardingProgressSaveError =>
      'Không thể lưu tiến độ thiết lập. Vui lòng thử lại.';

  @override
  String get nextAction => 'Tiếp theo';

  @override
  String get preflightTitle => 'Kiểm tra phần nền tảng';

  @override
  String get preflightBody =>
      'Bước này tách phần cục bộ bắt buộc khỏi các khả năng tùy chọn. Việc duyệt dung lượng và mạng cho Registry chỉ diễn ra sau khi có kế hoạch Init đã ký.';

  @override
  String get preflightRuntimeTitle => 'Runtime được bảo vệ';

  @override
  String get preflightRuntimeBody =>
      'Danh tính gắn với thiết bị, vault mã hóa và một tiến trình ghi Rust đã sẵn sàng.';

  @override
  String get preflightStorageTitle => 'Dữ liệu bắt buộc được tải riêng';

  @override
  String get preflightStorageBody =>
      'Ứng dụng không chứa bản phát hành Concept Registry. Bộ dữ liệu ban đầu được tải sau khi chạy app và có thể cần hơn 2 GB kể cả vùng làm việc.';

  @override
  String get preflightOptionalTitle => 'Các lane tùy chọn vẫn tắt';

  @override
  String get preflightOptionalBody =>
      'AI local hoặc cloud, thông báo và mạng node không bắt buộc để lưu raw draft riêng tư.';

  @override
  String get identityTitle => 'Bản cài đặt này là một node riêng';

  @override
  String get identityBody =>
      'OneBrain đã tạo các miền quyền Node, feed và Actor độc lập cho điện thoại này. Nó không replicate hay mở rộng node desktop.';

  @override
  String get identityReadyTitle => 'Quyền độc lập';

  @override
  String get identityReadyBody =>
      'Vật liệu ký riêng tư nằm sau ranh giới native và Rust. UI chỉ nhận các fact công khai có kiểu.';

  @override
  String get securityTitle => 'Mặc định riêng tư';

  @override
  String get securityBody =>
      'Nội dung capture bắt đầu ở PrivateLocal. Khi app vào nền, phiên riêng tư bị khóa; chuyển sang public hoặc mạng cần xác nhận riêng.';

  @override
  String get securityVaultTitle => 'Lưu trữ cục bộ mã hóa';

  @override
  String get securityVaultBody =>
      'Draft riêng tư và object riêng tư đã xác minh dùng kho gắn với thiết bị, bị loại khỏi backup hệ điều hành thông thường.';

  @override
  String get initHandoffTitle =>
      'Thêm dữ liệu Concept bắt buộc sau khi chạy app';

  @override
  String get initHandoffBody =>
      'Tra Concept, validation, encode KU, tìm Library và KQL cục bộ chưa khả dụng cho đến khi một bản Registry đã ký được xác minh và kích hoạt.';

  @override
  String get initHandoffLimitedTitle => 'Limited mode vẫn hữu ích';

  @override
  String get initHandoffLimitedBody =>
      'Bạn có thể capture và lưu raw draft văn bản mã hóa ngay. Init, Operations, storage và diagnostics vẫn truy cập được.';

  @override
  String get openInitAction => 'Mở Init dữ liệu bắt buộc';

  @override
  String get limitedModeAction => 'Tạm dùng Limited mode';

  @override
  String get initTitle => 'Dữ liệu Concept bắt buộc';

  @override
  String get initBody =>
      'Chưa có yêu cầu Registry nào được gửi. MOB-05 sẽ phân giải target đã ký và hiển thị byte, dung lượng, mạng và năng lượng chính xác trước khi tải lớn.';

  @override
  String get initBoundaryTitle => 'Tải sau khi app chạy';

  @override
  String get initBoundaryBody =>
      'concepts.obr và các index không bao giờ được đóng gói trong APK hay IPA. Màn hình này không giả lập rằng dữ liệu đã có.';

  @override
  String get initUnavailableAction => 'Bắt đầu Init';

  @override
  String get initUnavailableReason =>
      'Kế hoạch và truyền tải Registry đã ký chưa hoạt động trong bản build này.';

  @override
  String get homeTitle => 'Trang chủ';

  @override
  String get homeGreeting => 'Không gian tươi sáng cho ý tưởng riêng tư';

  @override
  String get limitedTitle => 'Limited mode';

  @override
  String get limitedBody =>
      'Node đã được bảo vệ nhưng dữ liệu Concept bắt buộc chưa active. Raw draft hoạt động; tính năng cần Concept được khóa với lý do rõ ràng.';

  @override
  String get requiredInitTitle => 'Hoàn tất dữ liệu bắt buộc';

  @override
  String get requiredInitBody =>
      'Mở Init để xem ranh giới tải dữ liệu sau khi chạy app. Thẻ này không tự bắt đầu truyền tải.';

  @override
  String get quickCaptureTitle => 'Ghi lại một ý tưởng';

  @override
  String get quickCaptureBody =>
      'Lưu văn bản có giới hạn thẳng vào kho draft PrivateLocal mã hóa. Không dùng LLM hay mạng.';

  @override
  String get captureAction => 'Ghi văn bản';

  @override
  String get draftCountTitle => 'Draft mã hóa';

  @override
  String draftCountBody(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: 'Có $count raw draft riêng tư trên thiết bị này.',
      one: 'Có 1 raw draft riêng tư trên thiết bị này.',
      zero: 'Chưa có raw draft riêng tư nào.',
    );
    return '$_temp0';
  }

  @override
  String get operationsTitle => 'Tác vụ';

  @override
  String get operationsBody =>
      'Không có tác vụ Registry, model, import, backup, sync hay seed đang hoạt động.';

  @override
  String get navHome => 'Trang chủ';

  @override
  String get navLibrary => 'Thư viện';

  @override
  String get navCapture => 'Ghi ý tưởng';

  @override
  String get navAssistant => 'Trợ lý';

  @override
  String get navSettings => 'Cài đặt';

  @override
  String get libraryTitle => 'Thư viện';

  @override
  String get libraryBody =>
      'Các kệ cục bộ tách riêng nguồn gốc, đường nhận, retention và trạng thái semantic. Node Limited sạch này chưa có bản Concept active.';

  @override
  String get myKnowledgeTitle => 'KU của tôi / cục bộ';

  @override
  String get myKnowledgeBody =>
      'Duyệt và xem KU riêng tư khả dụng sau khi Registry active và validation xác định hoàn tất.';

  @override
  String get receivedKnowledgeTitle => 'KU đã nhận';

  @override
  String get receivedKnowledgeBody =>
      'Kệ KU đã nhận cần gate Networked Mobile Beta; dữ liệu không được giả lập cục bộ.';

  @override
  String get mediaLibraryTitle => 'Media của tôi';

  @override
  String get mediaLibraryBody =>
      'Bản gốc sở hữu và media dẫn xuất sẽ dùng kho mã hóa đã xác minh. Ingestion media chưa active trong lát cắt này.';

  @override
  String get conceptsTitle => 'Concept, tìm kiếm và KQL';

  @override
  String get conceptsBody =>
      'Các route này cần một bản Concept Registry active, tương thích và khỏe mạnh.';

  @override
  String get registryRequiredReason =>
      'Dữ liệu Concept Registry bắt buộc chưa active.';

  @override
  String get networkBetaReason =>
      'Mạng node bị tắt cho đến gate Networked Mobile Beta.';

  @override
  String get captureTitle => 'Ghi ý tưởng';

  @override
  String get captureBody =>
      'Mọi nguồn bắt đầu ở PrivateLocal. Văn bản hay candidate dẫn xuất không ghi đè bản gốc sở hữu.';

  @override
  String get textCaptureTitle => 'Văn bản hoặc clipboard';

  @override
  String get textCaptureBody =>
      'Soạn văn bản có giới hạn và lưu thẳng vào kho raw draft mã hóa.';

  @override
  String get shareCaptureTitle => 'Chia sẻ vào OneBrain';

  @override
  String get shareCaptureBody =>
      'Ranh giới share-spool mã hóa có kiểu đã được dành chỗ; bản build này không nhận payload share chưa bảo vệ.';

  @override
  String get fileCaptureTitle => 'Ảnh, video, tài liệu hoặc audio';

  @override
  String get fileCaptureBody =>
      'System picker và staging media đã xác minh sẽ đến cùng gói lifecycle media.';

  @override
  String get textComposerTitle => 'Draft văn bản riêng tư';

  @override
  String get textComposerBody =>
      'Nguồn này chỉ lưu trên thiết bị. Lưu không đồng nghĩa encode KU, publish, share hay gọi AI.';

  @override
  String get contentLanguageLabel => 'Ngôn ngữ nội dung';

  @override
  String get draftTextLabel => 'Nội dung của bạn';

  @override
  String get draftTextHint => 'Viết hoặc dán một ý tưởng…';

  @override
  String get savePrivateDraftAction => 'Lưu draft riêng tư';

  @override
  String get draftSavedTitle => 'Đã lưu trên thiết bị này';

  @override
  String draftSavedBody(int bytes, int count) {
    return 'Đã lưu $bytes byte nguồn mã hóa. Kho riêng tư hiện có $count draft.';
  }

  @override
  String get draftSaveError =>
      'Không thể lưu draft riêng tư. Nội dung vẫn còn trong trình soạn thảo.';

  @override
  String get draftBlankError => 'Hãy nhập nội dung trước khi lưu.';

  @override
  String get assistantTitle => 'Trợ lý';

  @override
  String get assistantBody =>
      'Baseline xác định không-LLM được giữ nguyên. LLM local, hệ thống và cloud là các gói tùy chọn riêng, hiện đều tắt.';

  @override
  String get settingsTitle => 'Cài đặt';

  @override
  String get settingsBody =>
      'Xem runtime được bảo vệ, dữ liệu bắt buộc, storage và ranh giới khả năng tùy chọn.';

  @override
  String get runtimeSettingsTitle => 'Runtime và diagnostics';

  @override
  String get runtimeSettingsBody =>
      'Hồ sơ BootstrapOnly, danh tính được bảo vệ, một writer và lịch sử bảo mật đã lược bỏ dữ liệu nhạy cảm.';

  @override
  String get registrySettingsTitle => 'Concept Registry';

  @override
  String get registrySettingsBody =>
      'Chưa có bản phát hành active và chưa gửi yêu cầu Registry.';

  @override
  String get storageSettingsTitle => 'Lưu trữ';

  @override
  String get storageSettingsBody =>
      'Draft được bảo vệ, Registry, model, media, staging và byte có thể thu hồi luôn tách riêng.';

  @override
  String get backupSettingsTitle => 'Backup và export mã hóa';

  @override
  String get backupSettingsBody =>
      'Engine archive có phiên bản và xác thực đã có; việc nối đích do người dùng chọn vẫn đang gated.';

  @override
  String get languageSettingsTitle => 'Ngôn ngữ và trợ năng';

  @override
  String get languageSettingsBody =>
      'UI Anh/Việt, cỡ chữ hệ thống, tương phản và Reduce Motion dùng chung design contract.';

  @override
  String get unavailableTitle => 'Tính năng chưa khả dụng';

  @override
  String get backHomeAction => 'Về Trang chủ';
}
