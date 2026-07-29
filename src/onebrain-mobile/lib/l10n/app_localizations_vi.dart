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
    return 'Hồ sơ $profileVersion · thế hệ $generation · $phase với $grantCount quyền thực thi đang hoạt động. Trạng thái Registry: $registryState.';
  }

  @override
  String mobileRuntimeRecovered(int generation) {
    return 'Đã phục hồi thế hệ $generation sau khi tiến trình trước kết thúc mà không có callback quiesce. Callback cũ vẫn bị chặn.';
  }

  @override
  String get mobileRuntimeVerified =>
      'Đã xác minh KQL cục bộ có chữ ký, planner riêng tư và hàng rào callback';

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
}
